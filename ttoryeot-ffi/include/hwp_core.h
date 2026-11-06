#ifndef HWP_CORE_H
#define HWP_CORE_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define HWP_CORE_ABI_VERSION 1u

typedef struct {
    uint8_t *ptr;
    size_t len;
} HwpCoreBuf;

enum {
    HWP_CORE_HINT_AUTO = 0,
    HWP_CORE_HINT_HWP = 1,
    HWP_CORE_HINT_HWPX = 2
};

uint32_t hwp_core_abi_version(void);

/* 0: *out = frame (success or Fatal). -1: invalid arguments (*out = {NULL,0}).
   Free the frame only with hwp_core_buf_free. */
int32_t hwp_core_parse(const uint8_t *bytes, size_t len, uint32_t hint, HwpCoreBuf *out);
void hwp_core_buf_free(HwpCoreBuf buf);

#ifdef __cplusplus
}
#endif

#endif
