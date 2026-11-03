#ifndef PHISTUDIO_PLUGIN_H
#define PHISTUDIO_PLUGIN_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define PHISTUDIO_NATIVE_ABI_VERSION 1u
#define PHISTUDIO_API_VERSION 1u

typedef struct PhiStudioNativeHeader {
    uint32_t abi_version;
    uint32_t api_version;
    uint32_t struct_size;
} PhiStudioNativeHeader;

/* status 0: Response JSON; status 1: UTF-8 error. Buffers belong to the plugin. */
typedef struct PhiStudioNativeBuffer {
    uint8_t *data;
    size_t len;
    uint32_t status;
} PhiStudioNativeBuffer;

/* Input JSON is borrowed for this call. Calls are serialized. State may move
   between threads. No pointer may be retained and no exception may cross C.
   destroy must join plugin work; free receives each non-null output once. */
typedef struct PhiStudioNativeApiV1 {
    PhiStudioNativeHeader header;
    void *(*create)(void);
    void (*destroy)(void *state);
    PhiStudioNativeBuffer (*call)(void *state, const uint8_t *data, size_t len);
    void (*free)(uint8_t *data, size_t len);
} PhiStudioNativeApiV1;

/* The returned API table remains valid until the library is unloaded. */
const PhiStudioNativeApiV1 *phistudio_plugin_v1(void);

#ifdef __cplusplus
}
#endif
#endif
