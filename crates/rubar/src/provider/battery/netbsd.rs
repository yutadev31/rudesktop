use super::{BatteryState, BatteryStatus};

#[cfg(target_os = "netbsd")]
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Sample {
    charge: u64,
    full: u64,
    charging: bool,
    discharging: bool,
}

#[cfg(target_os = "netbsd")]
unsafe extern "C" {
    fn rubar_netbsd_batteries(
        samples: *mut Sample,
        capacity: usize,
        count: *mut usize,
        error: *mut std::ffi::c_char,
        error_len: usize,
    ) -> std::ffi::c_int;
}

#[cfg(target_os = "netbsd")]
pub struct EnvSys;

#[cfg(target_os = "netbsd")]
impl super::BatteryProvider for EnvSys {
    fn read(&mut self) -> Result<BatteryState, String> {
        let mut samples = [Sample::default(); 16];
        let mut count = 0;
        let mut error = [0 as std::ffi::c_char; 256];
        // SAFETY: all pointers refer to writable arrays of the lengths supplied.
        let result = unsafe {
            rubar_netbsd_batteries(
                samples.as_mut_ptr(),
                samples.len(),
                &mut count,
                error.as_mut_ptr(),
                error.len(),
            )
        };
        if result != 0 {
            // SAFETY: the C adapter always NUL-terminates the supplied error buffer.
            let reason = unsafe { std::ffi::CStr::from_ptr(error.as_ptr()) };
            return Err(format!(
                "could not read NetBSD battery: {}",
                reason.to_string_lossy()
            ));
        }
        aggregate(samples[..count].iter().map(|sample| {
            (
                sample.charge,
                sample.full,
                sample.charging,
                sample.discharging,
            )
        }))
    }
}

fn aggregate(
    samples: impl IntoIterator<Item = (u64, u64, bool, bool)>,
) -> Result<BatteryState, String> {
    let mut count = 0u64;
    let mut percent_sum = 0u64;
    let mut charging = false;
    let mut discharging = false;
    for (charge, full, is_charging, is_discharging) in samples {
        if full == 0 || charge > full {
            return Err("invalid NetBSD battery charge".to_string());
        }
        let percent = (u128::from(charge) * 100 + u128::from(full) / 2) / u128::from(full);
        percent_sum = percent_sum
            .checked_add(percent as u64)
            .ok_or("NetBSD battery percentage overflowed")?;
        count += 1;
        charging |= is_charging;
        discharging |= is_discharging;
    }
    if count == 0 {
        return Err("no battery found in envsys".to_string());
    }
    let percent = u8::try_from((percent_sum + count / 2) / count)
        .map_err(|_| "invalid NetBSD battery percentage")?;
    let status = if charging {
        BatteryStatus::Charging
    } else if discharging {
        BatteryStatus::Discharging
    } else if percent == 100 {
        BatteryStatus::Full
    } else {
        BatteryStatus::Unknown
    };
    Ok(BatteryState { percent, status })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aggregates_batteries_and_status() {
        assert_eq!(
            aggregate([(50, 100, false, true), (100, 100, false, false)]).unwrap(),
            BatteryState {
                percent: 75,
                status: BatteryStatus::Discharging
            }
        );
        assert_eq!(
            aggregate([(100, 100, true, false)]).unwrap().status,
            BatteryStatus::Charging
        );
        assert!(aggregate([]).is_err());
        assert!(aggregate([(101, 100, false, false)]).is_err());
    }
}
