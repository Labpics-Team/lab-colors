#include <errno.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

#include "hash.h"
#include "wire.h"

typedef struct {
    uint8_t *bytes;
    size_t length;
    size_t capacity;
    size_t maximum;
    bool limit_exceeded;
} byte_buffer;

typedef enum {
    LC_ARB_READ_OK = 0,
    LC_ARB_READ_EMPTY = 1,
    LC_ARB_READ_TOO_LARGE = 2,
    LC_ARB_READ_FAILED = 3
} lc_arb_read_status;

typedef enum {
    LC_ARB_EVALUATION_OK = 0,
    LC_ARB_EVALUATION_RESOURCE_LIMIT = 1,
    LC_ARB_EVALUATION_FAILED = 2
} lc_arb_evaluation_status;

static const uint8_t transcript_magic[8] = "LCTRN1\0";
static const uint8_t accounting_domain[] = "labcolors.arb-evaluation-accounting.v1\0";
static const uint8_t exact_trace_domain[] =
    "labcolors.proof-region.exact-zero-signal-trace.v1\0";
static const uint8_t boundary_enclosure_domain[] =
    "labcolors.arb-boundary-enclosure.v1\0";

static void
buffer_clear(byte_buffer *buffer)
{
    free(buffer->bytes);
    memset(buffer, 0, sizeof(*buffer));
}

static bool
buffer_reserve(byte_buffer *buffer, size_t additional)
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
buffer_append(byte_buffer *buffer, const uint8_t *bytes, size_t length)
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
buffer_u8(byte_buffer *buffer, uint8_t value)
{
    return buffer_append(buffer, &value, 1);
}

static bool
buffer_u32(byte_buffer *buffer, uint32_t value)
{
    uint8_t bytes[4];

    lc_write_u32_be(bytes, value);
    return buffer_append(buffer, bytes, sizeof(bytes));
}

static bool
buffer_u64(byte_buffer *buffer, uint64_t value)
{
    uint8_t bytes[8];

    lc_write_u64_be(bytes, value);
    return buffer_append(buffer, bytes, sizeof(bytes));
}

static lc_arb_read_status
read_stdin(byte_buffer *input)
{
    uint8_t chunk[16384];

    for (;;) {
        ssize_t count = read(STDIN_FILENO, chunk, sizeof(chunk));

        if (count < 0) {
            if (errno == EINTR) {
                continue;
            }
            return LC_ARB_READ_FAILED;
        }
        if (count == 0) {
            return input->length == 0 ? LC_ARB_READ_EMPTY : LC_ARB_READ_OK;
        }
        if (input->maximum != 0
            && (size_t) count > input->maximum - input->length) {
            return LC_ARB_READ_TOO_LARGE;
        }
        if (!buffer_append(input, chunk, (size_t) count)) {
            return LC_ARB_READ_FAILED;
        }
    }
}

static bool
digest_is_nonzero(const uint8_t digest[32])
{
    uint8_t aggregate = 0;

    for (size_t index = 0; index < 32; ++index) {
        aggregate |= digest[index];
    }
    return aggregate != 0;
}

static bool
parse_manifest_identity(const char *text, uint8_t identity[32])
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

static bool
exact_trace_digest(
    const lc_job *job,
    uint32_t ordinal,
    const lc_region_result *result,
    uint8_t digest[32]
)
{
    lc_sha256_context context;
    uint8_t encoded_ordinal[4];
    uint8_t encoded_branch[8];

    /*
     * Precision and enclosure belong to an engine run, not to the exact
     * signal.  Job, ordinal and the first exact branch select one replayable
     * mathematical trace identically for Arb and an independent comparator.
     */
    lc_write_u32_be(encoded_ordinal, ordinal);
    lc_write_u64_be(encoded_branch, result->exact_branch);
    lc_sha256_init(&context);
    lc_sha256_update(&context, exact_trace_domain, sizeof(exact_trace_domain) - 1);
    lc_sha256_update(&context, job->job_identity, 32);
    lc_sha256_update(&context, encoded_ordinal, sizeof(encoded_ordinal));
    lc_sha256_update(&context, encoded_branch, sizeof(encoded_branch));
    lc_sha256_finish(&context, digest);
    return digest_is_nonzero(digest);
}

static bool
boundary_enclosure_digest(
    const lc_job *job,
    uint32_t ordinal,
    uint32_t precision,
    const lc_region_result *result,
    uint8_t digest[32]
)
{
    lc_sha256_context context;
    uint8_t encoded[9];
    fmpz_t lower;
    fmpz_t upper;
    fmpz_t exponent;
    char *lower_text = NULL;
    char *upper_text = NULL;
    char *exponent_text = NULL;
    bool success = false;

    lc_write_u32_be(encoded, ordinal);
    lc_write_u32_be(encoded + 4, precision);
    encoded[8] = (uint8_t) result->formula_status;
    lc_sha256_init(&context);
    lc_sha256_update(
        &context,
        boundary_enclosure_domain,
        sizeof(boundary_enclosure_domain) - 1
    );
    lc_sha256_update(&context, job->job_identity, 32);
    lc_sha256_update(&context, encoded, sizeof(encoded));
    encoded[0] = result->has_enclosure ? 1 : 0;
    lc_sha256_update(&context, encoded, 1);
    if (result->has_enclosure) {
        uint8_t length[8];

        fmpz_init(lower);
        fmpz_init(upper);
        fmpz_init(exponent);
        lc_interval_get_dyadic_bounds(lower, upper, exponent, &result->enclosure);
        lower_text = fmpz_get_str(NULL, 16, lower);
        upper_text = fmpz_get_str(NULL, 16, upper);
        exponent_text = fmpz_get_str(NULL, 16, exponent);
        if (lower_text == NULL || upper_text == NULL || exponent_text == NULL) {
            goto cleanup;
        }
        const char *values[3] = {lower_text, upper_text, exponent_text};
        for (size_t index = 0; index < 3; ++index) {
            size_t text_length = strlen(values[index]);

            lc_write_u64_be(length, (uint64_t) text_length);
            lc_sha256_update(&context, length, sizeof(length));
            lc_sha256_update(&context, (const uint8_t *) values[index], text_length);
        }
    }
    lc_sha256_finish(&context, digest);
    success = digest_is_nonzero(digest);

cleanup:
    if (result->has_enclosure) {
        flint_free(exponent_text);
        flint_free(upper_text);
        flint_free(lower_text);
        fmpz_clear(exponent);
        fmpz_clear(upper);
        fmpz_clear(lower);
    }
    return success;
}

static void
account_point(
    lc_sha256_context *accounting,
    uint32_t ordinal,
    uint32_t precision,
    uint64_t consumed,
    lc_region_outcome outcome
)
{
    uint8_t record[17];

    lc_write_u32_be(record, ordinal);
    lc_write_u32_be(record + 4, precision);
    lc_write_u64_be(record + 8, consumed);
    record[16] = (uint8_t) outcome;
    lc_sha256_update(accounting, record, sizeof(record));
}

static bool
append_digest_witness(
    byte_buffer *witnesses,
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
    byte_buffer *witnesses,
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
lesser_u64(uint64_t left, uint64_t right)
{
    return left < right ? left : right;
}

static lc_arb_evaluation_status
evaluate(
    const lc_job *job,
    const uint8_t comparator_identity[32],
    byte_buffer *output
)
{
    byte_buffer decisions = {0};
    byte_buffer witnesses = {0};
    lc_domain_iterator iterator;
    lc_region_result result;
    lc_sha256_context accounting;
    uint64_t counters[4] = {0, 0, 0, 0};
    uint64_t equality_count = 0;
    uint64_t witness_count = 0;
    uint64_t global_remaining = job->policy.global_pregrant;
    uint8_t accounting_digest[32];
    size_t decision_length;
    lc_arb_evaluation_status status = LC_ARB_EVALUATION_FAILED;

    decisions.maximum = (size_t) LC_ARB_MAX_OUTPUT_BYTES_V1;
    witnesses.maximum = (size_t) LC_ARB_MAX_OUTPUT_BYTES_V1;

    if (job->domain.point_count == 0
        || job->policy.precision_count == 0
        || job->domain.point_count > SIZE_MAX - 3) {
        return LC_ARB_EVALUATION_FAILED;
    }
    decision_length = ((size_t) job->domain.point_count + 3) / 4;
    if (decision_length == 0
        || !buffer_reserve(&decisions, decision_length)
        || decisions.bytes == NULL) {
        status = decisions.limit_exceeded
            ? LC_ARB_EVALUATION_RESOURCE_LIMIT
            : LC_ARB_EVALUATION_FAILED;
        buffer_clear(&decisions);
        return status;
    }
    memset(decisions.bytes, 0, decision_length);
    decisions.length = decision_length;
    lc_sha256_init(&accounting);
    lc_sha256_update(&accounting, accounting_domain, sizeof(accounting_domain) - 1);
    lc_sha256_update(&accounting, job->job_identity, 32);
    lc_sha256_update(&accounting, job->domain.identity, 32);
    lc_sha256_update(&accounting, job->policy.identity, 32);
    lc_sha256_update(&accounting, comparator_identity, 32);
    lc_region_result_init(&result);
    lc_domain_iterator_init(&iterator, &job->domain);
    for (uint64_t point_index = 0; point_index < job->domain.point_count; ++point_index) {
        uint64_t point_grant = lesser_u64(
            job->policy.per_point_work,
            global_remaining
        );
        uint64_t point_remaining = point_grant;
        uint64_t point_consumed = 0;
        uint8_t resource_scope = job->policy.per_point_work <= global_remaining
            ? 1
            : 2;
        uint32_t ordinal;
        uint32_t final_precision = job->policy.precision_ladder[0];
        uint8_t rgb[3];

        /* A point owns its ordinal-prefix pregrant even when it uses none. */
        global_remaining -= point_grant;
        if (!lc_domain_iterator_next(&iterator, &ordinal)) {
            goto cleanup;
        }
        lc_ordinal_to_rgb(ordinal, rgb);
        for (size_t rung = 0; rung < job->policy.precision_count; ++rung) {
            uint64_t grant = point_remaining;

            final_precision = job->policy.precision_ladder[rung];
            lc_region_evaluate_rgb(
                &result,
                rgb,
                job->context,
                job->surround,
                &job->region,
                (slong) final_precision,
                grant
            );
            if (result.consumed_branches > grant
                || result.consumed_branches > point_remaining) {
                goto cleanup;
            }
            point_remaining -= result.consumed_branches;
            point_consumed += result.consumed_branches;
            if (result.outcome != LC_REGION_BOUNDARY_UNPROVEN) {
                break;
            }
        }
        if ((unsigned) result.outcome > LC_REGION_RESOURCE_LIMIT_REACHED) {
            goto cleanup;
        }
        decisions.bytes[point_index / 4] |= (uint8_t) result.outcome
            << (6U - 2U * (unsigned) (point_index % 4));
        ++counters[result.outcome];
        account_point(&accounting, ordinal, final_precision, point_consumed, result.outcome);
        if (result.outcome == LC_REGION_INSIDE && result.exact_boundary) {
            uint8_t digest[32];

            if (!exact_trace_digest(job, ordinal, &result, digest)
                || !append_digest_witness(&witnesses, 1, ordinal, digest)) {
                goto cleanup;
            }
            ++equality_count;
            ++witness_count;
        } else if (result.outcome == LC_REGION_BOUNDARY_UNPROVEN) {
            uint8_t digest[32];

            if (!boundary_enclosure_digest(
                    job,
                    ordinal,
                    final_precision,
                    &result,
                    digest
                )
                || !append_digest_witness(&witnesses, 2, ordinal, digest)) {
                goto cleanup;
            }
            ++witness_count;
        } else if (result.outcome == LC_REGION_RESOURCE_LIMIT_REACHED) {
            if (point_consumed != point_grant
                || !append_resource_witness(
                    &witnesses,
                    ordinal,
                    resource_scope,
                    point_grant
                )) {
                goto cleanup;
            }
            ++witness_count;
        }
    }
    lc_sha256_finish(&accounting, accounting_digest);
    if (!digest_is_nonzero(accounting_digest)
        || !buffer_append(output, transcript_magic, sizeof(transcript_magic))
        || !buffer_append(output, job->job_identity, 32)
        || !buffer_append(output, job->domain.identity, 32)
        || !buffer_append(output, comparator_identity, 32)
        || !buffer_u64(output, job->domain.point_count)
        || !buffer_u64(output, decisions.length)
        || !buffer_append(output, decisions.bytes, decisions.length)) {
        goto cleanup;
    }
    for (size_t index = 0; index < 4; ++index) {
        if (!buffer_u64(output, counters[index])) {
            goto cleanup;
        }
    }
    if (!buffer_u64(output, equality_count)
        || !buffer_append(output, accounting_digest, 32)
        || !buffer_u64(output, witness_count)
        || !buffer_append(output, witnesses.bytes, witnesses.length)) {
        goto cleanup;
    }
    status = LC_ARB_EVALUATION_OK;

cleanup:
    if (status != LC_ARB_EVALUATION_OK
        && (decisions.limit_exceeded
            || witnesses.limit_exceeded
            || output->limit_exceeded)) {
        status = LC_ARB_EVALUATION_RESOURCE_LIMIT;
    }
    lc_region_result_clear(&result);
    buffer_clear(&witnesses);
    buffer_clear(&decisions);
    return status;
}

int
main(int argc, char **argv)
{
    byte_buffer input = {0};
    byte_buffer output = {0};
    lc_job job;
    lc_wire_error error;
    uint8_t comparator_identity[32];
    int status = 1;
    lc_arb_read_status read_status;

    input.maximum = (size_t) LC_ARB_MAX_JOB_BYTES_V1;
    output.maximum = (size_t) LC_ARB_MAX_OUTPUT_BYTES_V1;

    if (argc != 5
        || strcmp(argv[1], "--manifest-identity") != 0
        || !parse_manifest_identity(argv[2], comparator_identity)
        || strcmp(argv[3], "--job") != 0
        || strcmp(argv[4], "/dev/stdin") != 0) {
        fputs(
            "usage: arb-evaluator --manifest-identity HEX64 --job /dev/stdin\n",
            stderr
        );
        return 64;
    }
    read_status = read_stdin(&input);
    if (read_status != LC_ARB_READ_OK) {
        const char *reason = read_status == LC_ARB_READ_TOO_LARGE
            ? "input_limit"
            : read_status == LC_ARB_READ_EMPTY ? "empty_input" : "io";

        fprintf(stderr, "job read failed: %s\n", reason);
        goto cleanup_input;
    }
    if (!lc_parse_job(&job, input.bytes, input.length, &error)) {
        fprintf(stderr, "job rejected: %s\n", lc_wire_error_name(error));
        goto cleanup_input;
    }
    lc_arb_evaluation_status evaluation =
        evaluate(&job, comparator_identity, &output);
    if (evaluation != LC_ARB_EVALUATION_OK) {
        fprintf(
            stderr,
            "evaluation failed: %s\n",
            evaluation == LC_ARB_EVALUATION_RESOURCE_LIMIT
                ? "output_limit"
                : "internal"
        );
        goto cleanup_job;
    }
    if (!lc_write_all(STDOUT_FILENO, output.bytes, output.length)) {
        fputs("result write failed\n", stderr);
        goto cleanup_job;
    }
    status = 0;

cleanup_job:
    buffer_clear(&output);
    lc_job_clear(&job);
cleanup_input:
    buffer_clear(&input);
    return status;
}
