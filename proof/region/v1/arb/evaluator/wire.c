#include "wire.h"

#include <errno.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

#include "hash.h"

typedef struct {
    const uint8_t *bytes;
    size_t length;
    size_t offset;
    lc_wire_error *error;
} reader;

static const uint8_t job_magic[8] = {'L', 'C', 'J', 'O', 'B', '1', 0, 0};
static const uint8_t domain_magic[8] = {'L', 'C', 'D', 'O', 'M', '1', 0, 0};
static const uint8_t policy_magic[8] = {'L', 'C', 'P', 'O', 'L', '1', 0, 0};
static const uint8_t definition_domain[] = "labcolors.contextual-region-family-provider.v1\0";
static const uint8_t formula_domain[] = "labcolors.nominal-exact-real-lift.ascii-ssa.v1\0";
static const uint8_t domain_identity_label[] = "labcolors.proof-region.domain.v1\0";
static const uint8_t policy_identity_label[] = "labcolors.proof-region.policy.v1\0";
static const uint8_t job_identity_label[] = "labcolors.proof-region.job.v1\0";
/* The registered V1 SSA has this exact wire length; changing either is a new
   formula release, never a permissive parser adjustment. */
static const size_t formula_spec_bytes_v1 = 24434;
static const uint8_t formula_release_v1[32] = {
    0x2c, 0x62, 0x6d, 0x8e, 0xe6, 0x0e, 0xeb, 0x62,
    0xae, 0x4d, 0xb5, 0x36, 0x60, 0xd6, 0x1b, 0xbc,
    0x25, 0xe0, 0xef, 0xd4, 0xe5, 0x57, 0xf0, 0xdc,
    0x1e, 0x77, 0x56, 0x5c, 0x13, 0x0b, 0x6e, 0x52,
};

static bool
reject(reader *input, lc_wire_error error)
{
    if (*input->error == LC_WIRE_OK) {
        *input->error = error;
    }
    return false;
}

static size_t
remaining(const reader *input)
{
    return input->length - input->offset;
}

static bool
take(reader *input, size_t length, lc_slice *slice)
{
    if (length > remaining(input)) {
        return reject(input, LC_WIRE_TRUNCATED);
    }
    slice->bytes = input->bytes + input->offset;
    slice->length = length;
    input->offset += length;
    return true;
}

static bool
expect(reader *input, const uint8_t *bytes, size_t length, lc_wire_error error)
{
    lc_slice actual;

    return take(input, length, &actual)
        && (memcmp(actual.bytes, bytes, length) == 0 || reject(input, error));
}

static bool
read_u8(reader *input, uint8_t *value)
{
    lc_slice bytes;

    if (!take(input, 1, &bytes)) {
        return false;
    }
    *value = bytes.bytes[0];
    return true;
}

static bool
read_u32(reader *input, uint32_t *value)
{
    lc_slice bytes;

    if (!take(input, 4, &bytes)) {
        return false;
    }
    *value = ((uint32_t) bytes.bytes[0] << 24)
        | ((uint32_t) bytes.bytes[1] << 16)
        | ((uint32_t) bytes.bytes[2] << 8)
        | (uint32_t) bytes.bytes[3];
    return true;
}

static bool
read_u64(reader *input, uint64_t *value)
{
    lc_slice bytes;
    uint64_t result = 0;

    if (!take(input, 8, &bytes)) {
        return false;
    }
    for (size_t index = 0; index < 8; ++index) {
        result = (result << 8) | bytes.bytes[index];
    }
    *value = result;
    return true;
}

static bool
read_blob(reader *input, size_t exact_length, lc_slice *value)
{
    uint64_t declared;

    if (!read_u64(input, &declared)) {
        return false;
    }
    if (declared > SIZE_MAX || (exact_length != SIZE_MAX && declared != exact_length)) {
        return reject(input, LC_WIRE_LENGTH_OUT_OF_BOUNDS);
    }
    if ((size_t) declared > remaining(input)) {
        return reject(input, LC_WIRE_LENGTH_OUT_OF_BOUNDS);
    }
    return take(input, (size_t) declared, value);
}

static bool
finish(reader *input)
{
    return remaining(input) == 0 || reject(input, LC_WIRE_TRAILING_BYTES);
}

void
lc_write_u32_be(uint8_t output[4], uint32_t value)
{
    output[0] = (uint8_t) (value >> 24);
    output[1] = (uint8_t) (value >> 16);
    output[2] = (uint8_t) (value >> 8);
    output[3] = (uint8_t) value;
}

void
lc_write_u64_be(uint8_t output[8], uint64_t value)
{
    for (size_t index = 0; index < 8; ++index) {
        output[7 - index] = (uint8_t) (value >> (index * 8));
    }
}

static void
content_identity(
    const uint8_t *label,
    size_t label_length,
    const uint8_t *bytes,
    size_t length,
    uint8_t digest[32]
)
{
    lc_sha256_context context;
    uint8_t encoded_length[8];

    lc_write_u64_be(encoded_length, (uint64_t) length);
    lc_sha256_init(&context);
    lc_sha256_update(&context, label, label_length);
    lc_sha256_update(&context, encoded_length, sizeof(encoded_length));
    lc_sha256_update(&context, bytes, length);
    lc_sha256_finish(&context, digest);
}

static bool
exact_bits(lc_slice field, arb_t output)
{
    uint64_t bits = 0;

    if (field.length != 8) {
        return false;
    }
    for (size_t index = 0; index < 8; ++index) {
        bits = (bits << 8) | field.bytes[index];
    }
    return lc_set_dyadic_bits(output, bits) == LC_OK;
}

static bool
is_one_byte(lc_slice field, uint8_t value)
{
    return field.length == 1 && field.bytes[0] == value;
}

static bool
parse_definition(lc_job *job, lc_slice encoded, reader *outer)
{
    static const size_t lengths[22] = {
        sizeof(definition_domain) - 1, 1, 1, 1, 1, 1, 1, 4, 1, 1, 4,
        8, 8, 1, 1, 1, 32, 1, 8, 8, 8, 8,
    };
    reader input = {encoded.bytes, encoded.length, 0, outer->error};
    lc_slice fields[22];
    uint64_t knot_count;
    arb_t determinant;
    arb_t product;
    arb_t one;

    for (size_t index = 0; index < 22; ++index) {
        if (!read_blob(&input, lengths[index], fields + index)) {
            return false;
        }
    }
    knot_count = 0;
    for (size_t index = 0; index < 8; ++index) {
        knot_count = (knot_count << 8) | fields[21].bytes[index];
    }
    if (knot_count == 0 || knot_count > SIZE_MAX / 64
        || remaining(&input) != (size_t) knot_count * 64) {
        return reject(&input, LC_WIRE_NONCANONICAL);
    }
    if (memcmp(fields[0].bytes, definition_domain, sizeof(definition_domain) - 1) != 0) {
        return reject(&input, LC_WIRE_UNKNOWN_RELEASE);
    }
    for (size_t index = 1; index <= 17; ++index) {
        bool fixed_one = index == 1 || index == 2 || index == 3 || index == 4
            || index == 5 || index == 6 || index == 8 || index == 9
            || index == 14 || index == 15 || index == 17;
        if (fixed_one && !is_one_byte(fields[index], 1)) {
            return reject(&input, LC_WIRE_UNKNOWN_RELEASE);
        }
    }
    if (memcmp(fields[7].bytes, "\x01\x01\x01\x01", 4) != 0
        || memcmp(fields[10].bytes, "\x01\x01\x01\x01", 4) != 0
        || fields[13].bytes[0] < 1 || fields[13].bytes[0] > 3
        || memcmp(fields[16].bytes, formula_release_v1, 32) != 0) {
        return reject(&input, LC_WIRE_UNKNOWN_RELEASE);
    }

    arb_init(job->context + 0);
    arb_init(job->context + 1);
    job->context_ready = true;
    if (!exact_bits(fields[11], job->context + 0)
        || !exact_bits(fields[12], job->context + 1)
        || !arb_is_positive(job->context + 0)
        || !arb_is_positive(job->context + 1)) {
        return reject(&input, LC_WIRE_NONCANONICAL);
    }
    arb_init(one);
    arb_one(one);
    if (!arb_le(job->context + 1, one)) {
        arb_clear(one);
        return reject(&input, LC_WIRE_NONCANONICAL);
    }
    arb_clear(one);
    job->surround = fields[13].bytes[0];
    memcpy(job->formula_release, fields[16].bytes, 32);

    if (!lc_region_init(&job->region, (size_t) knot_count)) {
        return reject(&input, LC_WIRE_ALLOCATION_FAILED);
    }
    job->region_ready = true;
    if (!exact_bits(fields[18], &job->region.metric_aa)
        || !exact_bits(fields[19], &job->region.metric_ab)
        || !exact_bits(fields[20], &job->region.metric_bb)
        || !arb_is_positive(&job->region.metric_aa)) {
        return reject(&input, LC_WIRE_NONCANONICAL);
    }
    arb_init(determinant);
    arb_init(product);
    /* Binary64 coordinates are exact dyadics. Exact precision keeps SPD
       admission independent of their exponent span. */
    arb_mul(
        determinant,
        &job->region.metric_aa,
        &job->region.metric_bb,
        ARF_PREC_EXACT
    );
    arb_mul(product, &job->region.metric_ab, &job->region.metric_ab, ARF_PREC_EXACT);
    arb_sub(determinant, determinant, product, ARF_PREC_EXACT);
    if (!arb_is_exact(determinant) || !arb_is_positive(determinant)) {
        arb_clear(product);
        arb_clear(determinant);
        return reject(&input, LC_WIRE_NONCANONICAL);
    }
    arb_clear(product);
    arb_clear(determinant);

    for (size_t index = 0; index < (size_t) knot_count; ++index) {
        lc_slice knot[4];
        lc_region_knot *target = job->region.knots + index;

        for (size_t coordinate = 0; coordinate < 4; ++coordinate) {
            if (!read_blob(&input, 8, knot + coordinate)) {
                return false;
            }
        }
        if (!exact_bits(knot[0], &target->tone)
            || !exact_bits(knot[1], &target->center_a)
            || !exact_bits(knot[2], &target->center_b)
            || !exact_bits(knot[3], &target->radius_squared)
            || !arb_is_nonnegative(&target->radius_squared)
            || (index != 0 && !arb_lt(&job->region.knots[index - 1].tone, &target->tone))) {
            return reject(&input, LC_WIRE_NONCANONICAL);
        }
    }
    return finish(&input);
}

static bool
parse_domain(lc_domain *domain, lc_slice encoded, const uint8_t expected[32], reader *outer)
{
    reader input = {encoded.bytes, encoded.length, 0, outer->error};
    uint8_t release;
    uint64_t range_count;
    uint64_t maximum;
    uint64_t total = 0;

    if (!expect(&input, domain_magic, sizeof(domain_magic), LC_WIRE_BAD_MAGIC)
        || !read_u8(&input, &release) || release != 1
        || !read_u64(&input, &domain->point_count)
        || domain->point_count == 0 || domain->point_count > UINT64_C(0x1000000)
        || !read_u64(&input, &range_count)) {
        return *input.error != LC_WIRE_OK
            ? false
            : reject(&input, LC_WIRE_NONCANONICAL);
    }
    maximum = domain->point_count;
    if (UINT64_C(0x1000001) - domain->point_count < maximum) {
        maximum = UINT64_C(0x1000001) - domain->point_count;
    }
    if (range_count == 0 || range_count > maximum || range_count > SIZE_MAX / sizeof(*domain->ranges)
        || range_count > remaining(&input) / 8 || (size_t) range_count * 8 != remaining(&input)) {
        return reject(&input, LC_WIRE_LENGTH_OUT_OF_BOUNDS);
    }
    domain->ranges = calloc((size_t) range_count, sizeof(*domain->ranges));
    if (domain->ranges == NULL) {
        return reject(&input, LC_WIRE_ALLOCATION_FAILED);
    }
    domain->range_count = (size_t) range_count;
    for (size_t index = 0; index < domain->range_count; ++index) {
        lc_ordinal_range *range = domain->ranges + index;

        if (!read_u32(&input, &range->start) || !read_u32(&input, &range->end)) {
            return false;
        }
        if (range->start >= range->end || range->end > UINT32_C(0x1000000)
            || (index != 0 && range->start <= domain->ranges[index - 1].end)) {
            return reject(&input, LC_WIRE_NONCANONICAL);
        }
        total += (uint64_t) range->end - range->start;
    }
    if (total != domain->point_count || !finish(&input)) {
        return *input.error != LC_WIRE_OK
            ? false
            : reject(&input, LC_WIRE_NONCANONICAL);
    }
    content_identity(
        domain_identity_label,
        sizeof(domain_identity_label) - 1,
        encoded.bytes,
        encoded.length,
        domain->identity
    );
    return memcmp(domain->identity, expected, 32) == 0
        || reject(&input, LC_WIRE_DIGEST_MISMATCH);
}

static bool
parse_policy(lc_arb_policy *policy, lc_slice encoded, const uint8_t expected[32], reader *outer)
{
    reader input = {encoded.bytes, encoded.length, 0, outer->error};
    uint8_t equality_release;
    uint8_t comparator_count;

    if (!expect(&input, policy_magic, sizeof(policy_magic), LC_WIRE_BAD_MAGIC)
        || !read_u8(&input, &equality_release)
        || !read_u8(&input, &comparator_count)) {
        return false;
    }
    if (equality_release != 1 || comparator_count != 2) {
        return reject(&input, LC_WIRE_UNKNOWN_RELEASE);
    }
    for (uint8_t expected_kind = 1; expected_kind <= 2; ++expected_kind) {
        uint8_t kind;
        uint32_t rung_count;
        uint32_t previous = 0;
        size_t minimum_tail;

        if (!read_u8(&input, &kind) || !read_u32(&input, &rung_count)) {
            return false;
        }
        minimum_tail = expected_kind == 1 ? 41 : 16;
        if (kind != expected_kind || rung_count == 0 || remaining(&input) < minimum_tail
            || rung_count > (remaining(&input) - minimum_tail) / 4) {
            return reject(&input, LC_WIRE_NONCANONICAL);
        }
        if (expected_kind == 1) {
            if ((size_t) rung_count > SIZE_MAX / sizeof(*policy->precision_ladder)) {
                return reject(&input, LC_WIRE_LENGTH_OUT_OF_BOUNDS);
            }
            policy->precision_ladder = calloc(rung_count, sizeof(*policy->precision_ladder));
            if (policy->precision_ladder == NULL) {
                return reject(&input, LC_WIRE_ALLOCATION_FAILED);
            }
            policy->precision_count = rung_count;
        }
        for (size_t index = 0; index < rung_count; ++index) {
            uint32_t precision;

            if (!read_u32(&input, &precision)) {
                return false;
            }
            if (precision == 0 || (index != 0 && precision <= previous)) {
                return reject(&input, LC_WIRE_NONCANONICAL);
            }
            if (expected_kind == 1) {
                policy->precision_ladder[index] = precision;
            }
            previous = precision;
        }
        if (expected_kind == 1) {
            if (!read_u64(&input, &policy->per_point_work)
                || !read_u64(&input, &policy->global_pregrant)) {
                return false;
            }
        } else {
            uint64_t ignored;

            if (!read_u64(&input, &ignored) || !read_u64(&input, &ignored)) {
                return false;
            }
        }
    }
    if (!finish(&input)) {
        return false;
    }
    content_identity(
        policy_identity_label,
        sizeof(policy_identity_label) - 1,
        encoded.bytes,
        encoded.length,
        policy->identity
    );
    return memcmp(policy->identity, expected, 32) == 0
        || reject(&input, LC_WIRE_DIGEST_MISMATCH);
}

static bool
formula_release(lc_slice formula, uint8_t digest[32])
{
    lc_sha256_context context;
    uint8_t length[8];

    lc_write_u64_be(length, (uint64_t) formula.length);
    lc_sha256_init(&context);
    lc_sha256_update(&context, formula_domain, sizeof(formula_domain) - 1);
    lc_sha256_update(&context, length, sizeof(length));
    lc_sha256_update(&context, formula.bytes, formula.length);
    lc_sha256_finish(&context, digest);
    return memcmp(digest, formula_release_v1, 32) == 0;
}

bool
lc_parse_job(
    lc_job *job,
    const uint8_t *bytes,
    size_t length,
    lc_wire_error *error
)
{
    reader input;
    lc_slice definition;
    lc_slice formula;
    lc_slice domain;
    lc_slice policy;
    lc_slice definition_digest;
    lc_slice declared_formula_release;
    lc_slice domain_identity;
    lc_slice policy_identity;
    uint8_t actual[32];

    memset(job, 0, sizeof(*job));
    *error = LC_WIRE_OK;
    input = (reader) {bytes, length, 0, error};
    if (!expect(&input, job_magic, sizeof(job_magic), LC_WIRE_BAD_MAGIC)
        || !take(&input, 32, &definition_digest)
        || !read_blob(&input, SIZE_MAX, &definition)
        || !take(&input, 32, &declared_formula_release)
        || !read_blob(&input, formula_spec_bytes_v1, &formula)
        || !take(&input, 32, &domain_identity)
        || !read_blob(&input, SIZE_MAX, &domain)
        || !take(&input, 32, &policy_identity)
        || !read_blob(&input, SIZE_MAX, &policy)
        || !finish(&input)) {
        lc_job_clear(job);
        return false;
    }
    lc_sha256(definition.bytes, definition.length, actual);
    if (memcmp(actual, definition_digest.bytes, 32) != 0) {
        *error = LC_WIRE_DIGEST_MISMATCH;
        lc_job_clear(job);
        return false;
    }
    if (!parse_definition(job, definition, &input)
        || memcmp(declared_formula_release.bytes, job->formula_release, 32) != 0
        || !formula_release(formula, actual)
        || memcmp(actual, declared_formula_release.bytes, 32) != 0
        || !parse_domain(&job->domain, domain, domain_identity.bytes, &input)
        || !parse_policy(&job->policy, policy, policy_identity.bytes, &input)) {
        if (*error == LC_WIRE_OK) {
            *error = LC_WIRE_DIGEST_MISMATCH;
        }
        lc_job_clear(job);
        return false;
    }
    content_identity(
        job_identity_label,
        sizeof(job_identity_label) - 1,
        bytes,
        length,
        job->job_identity
    );
    return true;
}

void
lc_job_clear(lc_job *job)
{
    free(job->policy.precision_ladder);
    free(job->domain.ranges);
    if (job->region_ready) {
        lc_region_clear(&job->region);
    }
    if (job->context_ready) {
        arb_clear(job->context + 1);
        arb_clear(job->context + 0);
    }
    memset(job, 0, sizeof(*job));
}

void
lc_domain_iterator_init(lc_domain_iterator *iterator, const lc_domain *domain)
{
    iterator->domain = domain;
    iterator->range_index = 0;
    iterator->ordinal = domain->ranges[0].start;
    iterator->emitted = 0;
}

bool
lc_domain_iterator_next(lc_domain_iterator *iterator, uint32_t *ordinal)
{
    if (iterator->emitted == iterator->domain->point_count) {
        return false;
    }
    *ordinal = iterator->ordinal;
    ++iterator->emitted;
    ++iterator->ordinal;
    if (iterator->ordinal == iterator->domain->ranges[iterator->range_index].end
        && iterator->emitted != iterator->domain->point_count) {
        /* Canonical parsing proves ordered disjoint ranges whose sizes sum to
           point_count, so remaining output implies that a next range exists. */
        ++iterator->range_index;
        iterator->ordinal = iterator->domain->ranges[iterator->range_index].start;
    }
    return true;
}

void
lc_ordinal_to_rgb(uint32_t ordinal, uint8_t rgb[3])
{
    rgb[0] = (uint8_t) (ordinal >> 16);
    rgb[1] = (uint8_t) (ordinal >> 8);
    rgb[2] = (uint8_t) ordinal;
}

const char *
lc_wire_error_name(lc_wire_error error)
{
    static const char *const names[] = {
        "ok",
        "truncated",
        "trailing_bytes",
        "length_out_of_bounds",
        "bad_magic",
        "unknown_release",
        "noncanonical",
        "digest_mismatch",
        "allocation_failed",
    };

    return (unsigned) error < sizeof(names) / sizeof(names[0])
        ? names[error]
        : "unknown_wire_error";
}

bool
lc_write_all(int descriptor, const uint8_t *bytes, size_t length)
{
    while (length != 0) {
        ssize_t written = write(descriptor, bytes, length);

        if (written < 0) {
            if (errno == EINTR) {
                continue;
            }
            return false;
        }
        if (written == 0) {
            return false;
        }
        bytes += (size_t) written;
        length -= (size_t) written;
    }
    return true;
}
