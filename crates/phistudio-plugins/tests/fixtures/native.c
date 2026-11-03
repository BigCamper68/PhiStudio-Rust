#include "phistudio_plugin.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#ifndef TEST_ABI
#define TEST_ABI PHISTUDIO_NATIVE_ABI_VERSION
#endif

static void record(const char *text) {
    FILE *file = fopen(TEST_LOG, "a");
    if (file) { fputs(text, file); fputc('\n', file); fclose(file); }
}
__attribute__((constructor)) static void loaded(void) { record("loaded"); }
__attribute__((destructor)) static void unloaded(void) { record("unloaded"); }

static void *create(void) { record("created"); return malloc(1); }
static void destroy(void *state) { record("destroyed"); free(state); }
static void release(uint8_t *data, size_t len) { (void)len; record("freed"); free(data); }

static PhiStudioNativeBuffer call(void *state, const uint8_t *data, size_t len) {
    (void)state;
    char *input = malloc(len + 1);
    if (!input) { return (PhiStudioNativeBuffer){NULL, 0, 1}; }
    memcpy(input, data, len); input[len] = '\0';
    const char *result = "{}";
    uint32_t status = 0;
    if (strstr(input, "\"kind\":\"activate\"")) {
        record("activated");
        result = "{\"contributions\":[{\"kind\":\"command\",\"id\":\"rename\",\"title\":\"Native rename\"}]}";
    } else if (strstr(input, "\"kind\":\"deactivate\"")) {
        record("deactivated");
    } else if (strstr(input, "\"arguments\":\"alias\"")) {
        record("aliased"); free(input);
        return (PhiStudioNativeBuffer){(uint8_t *)data, len, 0};
    } else if (strstr(input, "\"arguments\":\"oversized-alias\"")) {
        record("aliased"); free(input);
        return (PhiStudioNativeBuffer){(uint8_t *)data, 16 * 1024 * 1024 + 1, 0};
    } else if (strstr(input, "\"arguments\":\"oversized\"")) {
        record("oversized"); free(input);
        return (PhiStudioNativeBuffer){malloc(1), 16 * 1024 * 1024 + 1, 0};
    } else if (strstr(input, "\"arguments\":\"invalid-json\"")) {
        record("invalid-json"); result = "invalid JSON";
    } else if (strstr(input, "\"arguments\":\"fail\"")) {
        record("failed"); result = "Expected native failure"; status = 1;
    } else {
        record("command");
        result = "{\"effects\":[{\"kind\":\"edit\",\"transaction\":{\"version\":{\"session\":1,\"revision\":0},\"label\":\"Native rename\",\"operations\":[{\"op\":\"replace\",\"path\":\"/META/name\",\"value\":\"Native result\"}]}}]}";
    }
    free(input);
    size_t size = strlen(result);
    uint8_t *output = malloc(size);
    if (output) { memcpy(output, result, size); }
    return (PhiStudioNativeBuffer){output, output ? size : 0, status};
}

static const PhiStudioNativeApiV1 API = {
    {TEST_ABI, PHISTUDIO_API_VERSION, sizeof(PhiStudioNativeApiV1)},
    create, destroy, call, release
};
const PhiStudioNativeApiV1 *phistudio_plugin_v1(void) { return &API; }
