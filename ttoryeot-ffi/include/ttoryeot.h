#ifndef TTORYEOT_H
#define TTORYEOT_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define TTORYEOT_ABI_VERSION 1u

typedef struct {
    const uint8_t *ptr;
    size_t len;
} TtoryeotBytes;

typedef struct TtoryeotResult TtoryeotResult;

uint32_t ttoryeot_abi_version(void);

/* 입력은 호출 중에만 빌린다. 길이가 0이 아닌 모든 범위와 글꼴 배열은 호출이
   끝날 때까지 읽을 수 있어야 한다. password의 NULL 포인터는 암호 없음, NULL이 아닌
   길이 0은 빈 UTF-8 암호다. 글꼴은 TTF/OTF/TTC 원본 바이트다.
   0이면 결과를 돌려준다. 변환 실패도 결과 메타데이터에 담긴다.
   -1은 잘못된 인자이며 out이 NULL이 아니면 *out은 NULL이다.
   성공한 호출의 결과는 ttoryeot_result_free로 정확히 한 번 해제한다. */
int32_t ttoryeot_render(TtoryeotBytes document, TtoryeotBytes password,
                        const TtoryeotBytes *fonts, size_t font_count,
                        TtoryeotResult **out);

/* 반환된 바이트는 결과를 해제할 때까지만 유효하다. metadata는 UTF-8 JSON이다. */
TtoryeotBytes ttoryeot_result_pdf(const TtoryeotResult *result);
TtoryeotBytes ttoryeot_result_metadata(const TtoryeotResult *result);
void ttoryeot_result_free(TtoryeotResult *result);

#ifdef __cplusplus
}
#endif
#endif
