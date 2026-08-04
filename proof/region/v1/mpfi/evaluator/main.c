#include <errno.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

#include <mpfi.h>
#include "hash.h"
#include "wire.h"

typedef struct {
    uint8_t *bytes;
    size_t length;
    size_t capacity;
    size_t maximum;
    bool limit_exceeded;
} mpfi_buffer;

typedef enum {
    LC_MPFI_READ_OK = 0,
    LC_MPFI_READ_EMPTY = 1,
    LC_MPFI_READ_TOO_LARGE = 2,
    LC_MPFI_READ_FAILED = 3
} lc_mpfi_read_status;

typedef enum {
    LC_MPFI_EVALUATION_OK = 0,
    LC_MPFI_EVALUATION_RESOURCE_LIMIT = 1,
    LC_MPFI_EVALUATION_FAILED = 2
} lc_mpfi_evaluation_status;

static const uint8_t transcript_magic[8] = {'L', 'C', 'T', 'R', 'N', '1', 0, 0};
static const uint8_t accounting_domain[] =
    "labcolors.mpfi-evaluation-accounting.v1\0";
static const uint8_t exact_trace_domain[] =
    "labcolors.proof-region.exact-zero-signal-trace.v1\0";
static const uint8_t boundary_domain[] =
    "labcolors.mpfi-boundary-enclosure.v1\0";

static void
buffer_clear(mpfi_buffer *buffer)
{
    free(buffer->bytes);
    memset(buffer, 0, sizeof(*buffer));
}

static bool
buffer_reserve(mpfi_buffer *buffer, size_t additional)
{
    size_t required;
    size_t capacity;
    uint8_t *replacement;

    if (additional > SIZE_MAX - buffer->length) {
        return false;
    }
    required = buffer->length + additional;
    if (buffer->maximum != 0 && required > buffer->maximum) {
        buffer->limit_exceeded = true;
        return false;
    }
    if (required <= buffer->capacity) {
        return required == 0 || buffer->bytes != NULL;
    }
    capacity = buffer->capacity == 0 ? 4096 : buffer->capacity;
    while (capacity < required) {
        if (capacity > SIZE_MAX / 2) {
            capacity = required;
            break;
        }
        capacity *= 2;
    }
    replacement = realloc(buffer->bytes, capacity);
    if (replacement == NULL) {
        return false;
    }
    buffer->bytes = replacement;
    buffer->capacity = capacity;
    return true;
}

static bool
buffer_append(mpfi_buffer *buffer, const uint8_t *bytes, size_t length)
{
    if (length == 0) {
        return true;
    }
    if (!buffer_reserve(buffer, length)) {
        return false;
    }
    memcpy(buffer->bytes + buffer->length, bytes, length);
    buffer->length += length;
    return true;
}

static bool
buffer_u8(mpfi_buffer *buffer, uint8_t value)
{
    return buffer_append(buffer, &value, 1);
}

static bool
buffer_u32(mpfi_buffer *buffer, uint32_t value)
{
    uint8_t encoded[4];

    lc_mpfi_write_u32_be(encoded, value);
    return buffer_append(buffer, encoded, sizeof(encoded));
}

static bool
buffer_u64(mpfi_buffer *buffer, uint64_t value)
{
    uint8_t encoded[8];

    lc_mpfi_write_u64_be(encoded, value);
    return buffer_append(buffer, encoded, sizeof(encoded));
}

static lc_mpfi_read_status
read_stdin(mpfi_buffer *input)
{
    uint8_t chunk[16384];

    for (;;) {
        ssize_t count = read(STDIN_FILENO, chunk, sizeof(chunk));

        if (count < 0) {
            if (errno == EINTR) {
                continue;
            }
            return LC_MPFI_READ_FAILED;
        }
        if (count == 0) {
            return input->length == 0 ? LC_MPFI_READ_EMPTY : LC_MPFI_READ_OK;
        }
        if (input->maximum != 0
            && (size_t) count > input->maximum - input->length) {
            return LC_MPFI_READ_TOO_LARGE;
        }
        if (!buffer_append(input, chunk, (size_t) count)) {
            return LC_MPFI_READ_FAILED;
        }
    }
}

static bool
nonzero_digest(const uint8_t digest[32])
{
    uint8_t value = 0;

    for (size_t index = 0; index < 32; ++index) {
        value |= digest[index];
    }
    return value != 0;
}

static bool
parse_identity(const char *text, uint8_t identity[32])
{
    uint8_t aggregate = 0;

    if (strlen(text) != 64) {
        return false;
    }
    for (size_t index = 0; index < 32; ++index) {
        uint8_t value = 0;

        for (size_t nibble = 0; nibble < 2; ++nibble) {
            unsigned char character = (unsigned char) text[index * 2 + nibble];

            value <<= 4;
            if (character >= '0' && character <= '9') {
                value |= (uint8_t) (character - '0');
            } else if (character >= 'a' && character <= 'f') {
                value |= (uint8_t) (character - 'a' + 10);
            } else {
                return false;
            }
        }
        identity[index] = value;
        aggregate |= value;
    }
    return aggregate != 0;
}

static void
hash_common_prefix(
    lc_mpfi_sha256 *state,
    const uint8_t *domain,
    size_t domain_length,
    const lc_mpfi_job *job,
    uint32_t ordinal
)
{
    uint8_t ordinal_bytes[4];

    lc_mpfi_write_u32_be(ordinal_bytes, ordinal);
    lc_mpfi_sha256_init(state);
    lc_mpfi_sha256_update(state, domain, domain_length);
    lc_mpfi_sha256_update(state, job->job_identity, sizeof(job->job_identity));
    lc_mpfi_sha256_update(state, ordinal_bytes, sizeof(ordinal_bytes));
}

static bool
exact_trace_digest(
    const lc_mpfi_job *job,
    uint32_t ordinal,
    uint64_t exact_branch,
    uint8_t digest[32]
)
{
    lc_mpfi_sha256 state;
    uint8_t branch_bytes[8];

    hash_common_prefix(
        &state,
        exact_trace_domain,
        sizeof(exact_trace_domain) - 1,
        job,
        ordinal
    );
    lc_mpfi_write_u64_be(branch_bytes, exact_branch);
    lc_mpfi_sha256_update(&state, branch_bytes, sizeof(branch_bytes));
    lc_mpfi_sha256_finish(&state, digest);
    return nonzero_digest(digest);
}

static bool
hash_mpfr_string(lc_mpfi_sha256 *state, mpfr_srcptr value)
{
    mpfr_exp_t exponent;
    char *digits = mpfr_get_str(NULL, &exponent, 16, 0, value, MPFR_RNDN);
    uint8_t exponent_bytes[8];
    uint8_t length_bytes[8];
    size_t length;

    if (digits == NULL) {
        return false;
    }
    length = strlen(digits);
    lc_mpfi_write_u64_be(length_bytes, (uint64_t) length);
    lc_mpfi_write_u64_be(exponent_bytes, (uint64_t) exponent);
    lc_mpfi_sha256_update(state, exponent_bytes, sizeof(exponent_bytes));
    lc_mpfi_sha256_update(state, length_bytes, sizeof(length_bytes));
    lc_mpfi_sha256_update(state, (const uint8_t *) digits, length);
    mpfr_free_str(digits);
    return true;
}

static bool
boundary_digest(
    const lc_mpfi_job *job,
    uint32_t ordinal,
    uint32_t precision,
    const lc_mpfi_region_result *result,
    uint8_t digest[32]
)
{
    lc_mpfi_sha256 state;
    uint8_t precision_bytes[4];
    uint8_t status = (uint8_t) result->formula_status;
    uint8_t present = result->has_enclosure ? 1 : 0;
    __mpfr_struct lower;
    __mpfr_struct upper;
    bool success = false;

    hash_common_prefix(&state, boundary_domain, sizeof(boundary_domain) - 1, job, ordinal);
    lc_mpfi_write_u32_be(precision_bytes, precision);
    lc_mpfi_sha256_update(&state, precision_bytes, sizeof(precision_bytes));
    lc_mpfi_sha256_update(&state, &status, sizeof(status));
    lc_mpfi_sha256_update(&state, &present, sizeof(present));
    if (result->has_enclosure) {
        mpfr_init2(&lower, result->precision);
        mpfr_init2(&upper, result->precision);
        mpfi_get_left(&lower, &result->enclosure);
        mpfi_get_right(&upper, &result->enclosure);
        success = hash_mpfr_string(&state, &lower)
            && hash_mpfr_string(&state, &upper);
        mpfr_clear(&upper);
        mpfr_clear(&lower);
    } else {
        success = true;
    }
    if (!success) {
        return false;
    }
    lc_mpfi_sha256_finish(&state, digest);
    return nonzero_digest(digest);
}

static void
account_point(
    lc_mpfi_sha256 *state,
    uint32_t ordinal,
    uint32_t precision,
    uint64_t consumed,
    lc_mpfi_region_outcome outcome
)
{
    uint8_t record[17];

    lc_mpfi_write_u32_be(record, ordinal);
    lc_mpfi_write_u32_be(record + 4, precision);
    lc_mpfi_write_u64_be(record + 8, consumed);
    record[16] = (uint8_t) outcome;
    lc_mpfi_sha256_update(state, record, sizeof(record));
}

static bool
append_digest_witness(
    mpfi_buffer *witnesses,
    uint8_t kind,
    uint32_t ordinal,
    const uint8_t digest[32]
)
{
    return buffer_u8(witnesses, kind)
        && buffer_u32(witnesses, ordinal)
        && buffer_append(witnesses, digest, 32);
}

static bool
append_resource_witness(
    mpfi_buffer *witnesses,
    uint32_t ordinal,
    uint8_t scope,
    uint64_t grant
)
{
    return buffer_u8(witnesses, 3)
        && buffer_u32(witnesses, ordinal)
        && buffer_u8(witnesses, scope)
        && buffer_u64(witnesses, grant)
        && buffer_u64(witnesses, grant);
}

static uint64_t
smaller(uint64_t left, uint64_t right)
{
    return left < right ? left : right;
}

static lc_mpfi_evaluation_status
evaluate_job(
    const lc_mpfi_job *job,
    const uint8_t comparator_identity[32],
    mpfi_buffer *output
)
{
    mpfi_buffer decisions = {0};
    mpfi_buffer witnesses = {0};
    lc_mpfi_domain_iterator iterator;
    lc_mpfi_region_result result;
    lc_mpfi_sha256 accounting;
    uint64_t counters[4] = {0, 0, 0, 0};
    uint64_t equality_count = 0;
    uint64_t witness_count = 0;
    uint64_t remaining_global = job->policy.global_pregrant;
    uint8_t accounting_digest[32];
    lc_mpfi_evaluation_status status = LC_MPFI_EVALUATION_FAILED;

    decisions.maximum = (size_t) LC_MPFI_MAX_OUTPUT_BYTES_V1;
    witnesses.maximum = (size_t) LC_MPFI_MAX_OUTPUT_BYTES_V1;

    if (job->domain.point_count == 0
        || job->policy.precision_count == 0
        || job->domain.point_count > SIZE_MAX - 3
        || !lc_mpfi_region_result_init(&result, job->maximum_precision)) {
        return LC_MPFI_EVALUATION_FAILED;
    }
    size_t decision_length = ((size_t) job->domain.point_count + 3) / 4;
    if (decision_length == 0
        || !buffer_reserve(&decisions, decision_length)
        || decisions.bytes == NULL) {
        goto cleanup_buffers;
    }
    memset(decisions.bytes, 0, decision_length);
    decisions.length = decision_length;
    lc_mpfi_sha256_init(&accounting);
    lc_mpfi_sha256_update(
        &accounting,
        accounting_domain,
        sizeof(accounting_domain) - 1
    );
    lc_mpfi_sha256_update(&accounting, job->job_identity, 32);
    lc_mpfi_sha256_update(&accounting, job->domain.identity, 32);
    lc_mpfi_sha256_update(&accounting, job->policy.identity, 32);
    lc_mpfi_sha256_update(&accounting, comparator_identity, 32);
    lc_mpfi_domain_iterator_init(&iterator, &job->domain);
    for (uint64_t point_index = 0; point_index < job->domain.point_count; ++point_index) {
        uint64_t point_grant = smaller(
            job->policy.per_point_work,
            remaining_global
        );
        uint64_t point_remaining = point_grant;
        uint64_t point_consumed = 0;
        uint8_t scope = job->policy.per_point_work <= remaining_global ? 1 : 2;
        uint32_t ordinal;
        uint32_t final_precision = job->policy.precision_ladder[0];
        uint8_t rgb[3];

        remaining_global -= point_grant;
        if (!lc_mpfi_domain_iterator_next(&iterator, &ordinal)) {
            goto cleanup_buffers;
        }
        lc_mpfi_ordinal_to_rgb(ordinal, rgb);
        for (size_t rung = 0; rung < job->policy.precision_count; ++rung) {
            uint64_t grant = point_remaining;

            final_precision = job->policy.precision_ladder[rung];
            lc_mpfi_region_evaluate_rgb(
                &result,
                rgb,
                job->context,
                job->surround,
                &job->region,
                final_precision,
                grant
            );
            if (result.consumed_branches > grant
                || result.consumed_branches > point_remaining) {
                goto cleanup_buffers;
            }
            point_remaining -= result.consumed_branches;
            point_consumed += result.consumed_branches;
            if (result.outcome != LC_MPFI_REGION_BOUNDARY_UNPROVEN) {
                break;
            }
        }
        if ((unsigned) result.outcome > LC_MPFI_REGION_RESOURCE_LIMIT_REACHED) {
            goto cleanup_buffers;
        }
        decisions.bytes[point_index / 4] |=
            (uint8_t) result.outcome << (6U - 2U * (unsigned) (point_index % 4));
        ++counters[result.outcome];
        account_point(
            &accounting,
            ordinal,
            final_precision,
            point_consumed,
            result.outcome
        );
        if (result.outcome == LC_MPFI_REGION_INSIDE && result.exact_boundary) {
            uint8_t digest[32];

            if (!exact_trace_digest(job, ordinal, result.exact_branch, digest)
                || !append_digest_witness(&witnesses, 1, ordinal, digest)) {
                goto cleanup_buffers;
            }
            ++equality_count;
            ++witness_count;
        } else if (result.outcome == LC_MPFI_REGION_BOUNDARY_UNPROVEN) {
            uint8_t digest[32];

            if (!boundary_digest(job, ordinal, final_precision, &result, digest)
                || !append_digest_witness(&witnesses, 2, ordinal, digest)) {
                goto cleanup_buffers;
            }
            ++witness_count;
        } else if (result.outcome == LC_MPFI_REGION_RESOURCE_LIMIT_REACHED) {
            if (point_consumed != point_grant
                || !append_resource_witness(&witnesses, ordinal, scope, point_grant)) {
                goto cleanup_buffers;
            }
            ++witness_count;
        }
    }
    lc_mpfi_sha256_finish(&accounting, accounting_digest);
    if (!nonzero_digest(accounting_digest)
        || !buffer_append(output, transcript_magic, sizeof(transcript_magic))
        || !buffer_append(output, job->job_identity, 32)
        || !buffer_append(output, job->domain.identity, 32)
        || !buffer_append(output, comparator_identity, 32)
        || !buffer_u64(output, job->domain.point_count)
        || !buffer_u64(output, decisions.length)
        || !buffer_append(output, decisions.bytes, decisions.length)) {
        goto cleanup_buffers;
    }
    for (size_t index = 0; index < 4; ++index) {
        if (!buffer_u64(output, counters[index])) {
            goto cleanup_buffers;
        }
    }
    if (!buffer_u64(output, equality_count)
        || !buffer_append(output, accounting_digest, sizeof(accounting_digest))
        || !buffer_u64(output, witness_count)
        || !buffer_append(output, witnesses.bytes, witnesses.length)) {
        goto cleanup_buffers;
    }
    status = LC_MPFI_EVALUATION_OK;

cleanup_buffers:
    if (status != LC_MPFI_EVALUATION_OK
        && (decisions.limit_exceeded
            || witnesses.limit_exceeded
            || output->limit_exceeded)) {
        status = LC_MPFI_EVALUATION_RESOURCE_LIMIT;
    }
    buffer_clear(&witnesses);
    buffer_clear(&decisions);
    lc_mpfi_region_result_clear(&result);
    return status;
}

int
main(int argc, char **argv)
{
    mpfi_buffer input = {0};
    mpfi_buffer output = {0};
    lc_mpfi_job job;
    lc_mpfi_wire_error error;
    uint8_t comparator_identity[32];
    int status = 1;
    lc_mpfi_read_status read_status;

    input.maximum = (size_t) LC_MPFI_MAX_JOB_BYTES_V1;
    output.maximum = (size_t) LC_MPFI_MAX_OUTPUT_BYTES_V1;

    if (argc != 5
        || strcmp(argv[1], "--manifest-identity") != 0
        || !parse_identity(argv[2], comparator_identity)
        || strcmp(argv[3], "--job") != 0
        || strcmp(argv[4], "/dev/stdin") != 0) {
        fputs(
            "usage: mpfi-evaluator --manifest-identity HEX64 --job /dev/stdin\n",
            stderr
        );
        return 64;
    }
    read_status = read_stdin(&input);
    if (read_status != LC_MPFI_READ_OK) {
        const char *reason = read_status == LC_MPFI_READ_TOO_LARGE
            ? "input_limit"
            : read_status == LC_MPFI_READ_EMPTY ? "empty_input" : "io";

        fprintf(stderr, "job read failed: %s\n", reason);
        goto cleanup_input;
    }
    if (!lc_mpfi_parse_job(&job, input.bytes, input.length, &error)) {
        fprintf(stderr, "job rejected: %s\n", lc_mpfi_wire_error_name(error));
        goto cleanup_input;
    }
    lc_mpfi_evaluation_status evaluation =
        evaluate_job(&job, comparator_identity, &output);
    if (evaluation != LC_MPFI_EVALUATION_OK) {
        fprintf(
            stderr,
            "evaluation failed: %s\n",
            evaluation == LC_MPFI_EVALUATION_RESOURCE_LIMIT
                ? "output_limit"
                : "internal"
        );
        goto cleanup_job;
    }
    if (!lc_mpfi_write_all(STDOUT_FILENO, output.bytes, output.length)) {
        fputs("result write failed\n", stderr);
        goto cleanup_job;
    }
    status = 0;

cleanup_job:
    buffer_clear(&output);
    lc_mpfi_job_clear(&job);
cleanup_input:
    buffer_clear(&input);
    return status;
}
