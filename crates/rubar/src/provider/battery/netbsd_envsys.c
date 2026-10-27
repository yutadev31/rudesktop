#include <sys/envsys.h>

#include <errno.h>
#include <fcntl.h>
#include <prop/proplib.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <unistd.h>

struct rubar_battery_sample {
    uint64_t charge;
    uint64_t full;
    bool charging;
    bool discharging;
};

static void
set_error(char *error, size_t error_len, const char *name, const char *reason)
{
    if (error_len != 0)
        (void)snprintf(error, error_len, "%s: %s", name, reason);
}

static bool
sensor_name_is(prop_dictionary_t sensor, const char *name)
{
    const char *description;
    return prop_dictionary_get_cstring_nocopy(sensor, "description", &description)
        && strcmp(description, name) == 0;
}

static int
read_battery(prop_array_t sensors, const char *name,
    struct rubar_battery_sample *sample, char *error, size_t error_len)
{
    bool is_battery = false;
    bool present = true;
    bool found_charge = false;
    int64_t charge = 0;
    int64_t full = 0;
    memset(sample, 0, sizeof(*sample));

    for (unsigned int index = 0; index < prop_array_count(sensors); ++index) {
        prop_dictionary_t sensor = (prop_dictionary_t)prop_array_get(sensors, index);
        prop_dictionary_t properties = (prop_dictionary_t)prop_dictionary_get(sensor,
            "device-properties");
        if (properties != NULL) {
            const char *device_class;
            if (prop_dictionary_get_cstring_nocopy(properties, "device-class",
                &device_class) && strcmp(device_class, "battery") == 0)
                is_battery = true;
            continue;
        }
        int64_t value;
        if (sensor_name_is(sensor, "present") &&
            prop_dictionary_get_int64(sensor, "cur-value", &value)) {
            present = value != 0;
        } else if (sensor_name_is(sensor, "charge")) {
            const char *state;
            if (prop_dictionary_get_cstring_nocopy(sensor, "state", &state) &&
                strcmp(state, "valid") != 0)
                continue;
            if (prop_dictionary_get_int64(sensor, "cur-value", &charge) &&
                prop_dictionary_get_int64(sensor, "max-value", &full))
                found_charge = true;
        } else if (sensor_name_is(sensor, "charging") &&
            prop_dictionary_get_int64(sensor, "cur-value", &value)) {
            sample->charging = value > 0;
        } else if (sensor_name_is(sensor, "discharge rate") &&
            prop_dictionary_get_int64(sensor, "cur-value", &value)) {
            sample->discharging = value > 0;
        }
    }

    if (!is_battery || !present)
        return 0;
    if (!found_charge || full <= 0 || charge < 0 || charge > full) {
        set_error(error, error_len, name, "invalid or missing charge sensor");
        return -1;
    }
    sample->charge = (uint64_t)charge;
    sample->full = (uint64_t)full;
    return 1;
}

int
rubar_netbsd_batteries(struct rubar_battery_sample *samples, size_t capacity,
    size_t *count, char *error, size_t error_len)
{
    int fd = open("/dev/sysmon", O_RDONLY);
    if (fd == -1) {
        set_error(error, error_len, "/dev/sysmon", strerror(errno));
        return -1;
    }

    prop_dictionary_t devices = NULL;
    int result = prop_dictionary_recv_ioctl(fd, ENVSYS_GETDICTIONARY, &devices);
    (void)close(fd);
    if (result != 0) {
        set_error(error, error_len, "envsys", strerror(result));
        return -1;
    }

    prop_object_iterator_t iterator = prop_dictionary_iterator(devices);
    if (iterator == NULL) {
        set_error(error, error_len, "envsys", "could not iterate devices");
        prop_object_release(devices);
        return -1;
    }

    *count = 0;
    prop_object_t key;
    while ((key = prop_object_iterator_next(iterator)) != NULL) {
        const char *name = prop_dictionary_keysym_cstring_nocopy((prop_dictionary_keysym_t)key);
        prop_array_t sensors = (prop_array_t)prop_dictionary_get_keysym(devices,
            (prop_dictionary_keysym_t)key);
        struct rubar_battery_sample sample;
        int found = read_battery(sensors, name, &sample, error, error_len);
        if (found < 0) {
            result = -1;
            break;
        }
        if (found > 0) {
            if (*count == capacity) {
                set_error(error, error_len, "envsys", "too many batteries");
                result = -1;
                break;
            }
            samples[(*count)++] = sample;
        }
    }

    prop_object_iterator_release(iterator);
    prop_object_release(devices);
    return result;
}
