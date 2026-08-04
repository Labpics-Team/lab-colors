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
    lc_mpfi_wire_error *error;
} mpfi_reader;

static const uint8_t job_magic[8] = {'L', 'C', 'J', 'O', 'B', '1', 0, 0};
static const uint8_t domain_magic[8] = {'L', 'C', 'D', 'O', 'M', '1', 0, 0};
static const uint8_t policy_magic[8] = {'L', 'C', 'P', 'O', 'L', '1', 0, 0};
static const uint8_t definition_domain[] =
    "labcolors.contextual-region-family-provider.v1\0";
static const uint8_t formula_domain[] =
    "labcolors.nominal-exact-real-lift.ascii-ssa.v1\0";
static const uint8_t domain_identity_label[] =
    "labcolors.proof-region.domain.v1\0";
static const uint8_t policy_identity_label[] =
    "labcolors.proof-region.policy.v1\0";
static const uint8_t job_identity_label[] =
    "labcolors.proof-region.job.v1\0";
static const size_t formula_spec_length = 24434;
static const uint8_t formula_release_v1[32] = {
    0x2c, 0x62, 0x6d, 0x8e, 0xe6, 0x0e, 0xeb, 0x62,
    0xae, 0x4d, 0xb5, 0x36, 0x60, 0xd6, 0x1b, 0xbc,
    0x25, 0xe0, 0xef, 0xd4, 0xe5, 0x57, 0xf0, 0xdc,
    0x1e, 0x77, 0x56, 0x5c, 0x13, 0x0b, 0x6e, 0x52,
};

static bool
reject(mpfi_reader *input, lc_mpfi_wire_error error)
{
    if (*input->error == LC_MPFI_WIRE_OK) {
        *input->error = error;
    }
    return false;
}

static size_t
available(const mpfi_reader *input)
{
    return input->length - input->offset;
}

static bool
take(mpfi_reader *input, size_t length, lc_mpfi_slice *slice)
{
    if (length > available(input)) {
        return reject(input, LC_MPFI_WIRE_TRUNCATED);
    }
    slice->bytes = input->bytes + input->offset;
    slice->length = length;
    input->offset += length;
    return true;
}

static bool
expect(
    mpfi_reader *input,
    const uint8_t *expected,
    size_t length,
    lc_mpfi_wire_error error
)
{
    lc_mpfi_slice actual;

    return take(input, length, &actual)
        && (memcmp(actual.bytes, expected, length) == 0 || reject(input, error));
}

static bool
read_u8(mpfi_reader *input, uint8_t *value)
{
    lc_mpfi_slice byte;

    if (!take(input, 1, &byte)) {
        return false;
    }
    *value = byte.bytes[0];
    return true;
}

static bool
read_u32(mpfi_reader *input, uint32_t *value)
{
    lc_mpfi_slice bytes;

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
read_u64(mpfi_reader *input, uint64_t *value)
{
    lc_mpfi_slice bytes;
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
read_blob(mpfi_reader *input, size_t exact_length, lc_mpfi_slice *value)
{
    uint64_t declared;

    if (!read_u64(input, &declared)) {
        return false;
    }
    if (declared > SIZE_MAX
        || (exact_length != SIZE_MAX && declared != exact_length)) {
        return reject(input, LC_MPFI_WIRE_LENGTH_OUT_OF_BOUNDS);
    }
    if ((size_t) declared > available(input)) {
        return reject(input, LC_MPFI_WIRE_LENGTH_OUT_OF_BOUNDS);
    }
    return take(input, (size_t) declared, value);
}

static bool
finish(mpfi_reader *input)
{
    return available(input) == 0
        || reject(input, LC_MPFI_WIRE_TRAILING_BYTES);
}

void
lc_mpfi_write_u32_be(uint8_t output[4], uint32_t value)
{
    output[0] = (uint8_t) (value >> 24);
    output[1] = (uint8_t) (value >> 16);
    output[2] = (uint8_t) (value >> 8);
    output[3] = (uint8_t) value;
}

void
lc_mpfi_write_u64_be(uint8_t output[8], uint64_t value)
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
    lc_mpfi_sha256 state;
    uint8_t encoded_length[8];

    lc_mpfi_write_u64_be(encoded_length, (uint64_t) length);
    lc_mpfi_sha256_init(&state);
    lc_mpfi_sha256_update(&state, label, label_length);
    lc_mpfi_sha256_update(&state, encoded_length, sizeof(encoded_length));
    lc_mpfi_sha256_update(&state, bytes, length);
    lc_mpfi_sha256_finish(&state, digest);
}

static bool
decode_bits(lc_mpfi_slice field, uint64_t *bits)
{
    uint64_t value = 0;

    if (field.length != 8) {
        return false;
    }
    for (size_t index = 0; index < 8; ++index) {
        value = (value << 8) | field.bytes[index];
    }
    if ((value >> 52 & UINT64_C(0x7ff)) == UINT64_C(0x7ff)
        || value == UINT64_C(0x8000000000000000)) {
        return false;
    }
    *bits = value;
    return true;
}

static bool
bits_to_rational(uint64_t bits, mpq_t result)
{
    uint64_t exponent_bits = (bits >> 52) & UINT64_C(0x7ff);
    uint64_t significand = bits & UINT64_C(0x000fffffffffffff);
    long exponent;
    mpz_t numerator;
    mpz_t denominator;

    if (exponent_bits == UINT64_C(0x7ff)
        || bits == UINT64_C(0x8000000000000000)) {
        return false;
    }
    if (exponent_bits == 0) {
        exponent = -1074;
    } else {
        significand |= UINT64_C(0x0010000000000000);
        exponent = (long) exponent_bits - 1075;
    }
    mpz_init_set_ui(numerator, significand);
    mpz_init_set_ui(denominator, 1);
    if ((bits >> 63) != 0 && significand != 0) {
        mpz_neg(numerator, numerator);
    }
    if (exponent >= 0) {
        mpz_mul_2exp(numerator, numerator, (unsigned long) exponent);
    } else {
        mpz_mul_2exp(denominator, denominator, (unsigned long) -exponent);
    }
    mpq_init(result);
    mpq_set_num(result, numerator);
    mpq_set_den(result, denominator);
    mpq_canonicalize(result);
    mpz_clear(denominator);
    mpz_clear(numerator);
    return true;
}

static bool
field_rational(lc_mpfi_slice field, mpq_t result)
{
    uint64_t bits;

    return decode_bits(field, &bits) && bits_to_rational(bits, result);
}

static bool
field_to_interval(lc_mpfi_slice field, mpfi_ptr output)
{
    uint64_t bits;

    if (!decode_bits(field, &bits)) {
        return false;
    }
    return lc_mpfi_set_dyadic_bits(output, bits) == LC_MPFI_OK;
}

static bool
fixed_one(lc_mpfi_slice field)
{
    return field.length == 1 && field.bytes[0] == 1;
}

static bool
parse_definition(
    lc_mpfi_job *job,
    lc_mpfi_slice encoded,
    mpfr_prec_t precision,
    lc_mpfi_wire_error *error
)
{
    static const size_t prefix_lengths[22] = {
        sizeof(definition_domain) - 1, 1, 1, 1, 1, 1, 1, 4, 1, 1, 4,
        8, 8, 1, 1, 1, 32, 1, 8, 8, 8, 8,
    };
    mpfi_reader input = {encoded.bytes, encoded.length, 0, error};
    lc_mpfi_slice fields[22];
    uint64_t knot_count;
    mpq_t adapting;
    mpq_t background;
    mpq_t metric_aa;
    mpq_t metric_ab;
    mpq_t metric_bb;
    mpq_t determinant;
    mpq_t product;

    for (size_t index = 0; index < 22; ++index) {
        if (!read_blob(&input, prefix_lengths[index], fields + index)) {
            return false;
        }
    }
    knot_count = 0;
    for (size_t index = 0; index < 8; ++index) {
        knot_count = (knot_count << 8) | fields[21].bytes[index];
    }
    if (knot_count == 0) {
        return reject(&input, LC_MPFI_WIRE_NONCANONICAL);
    }
    if (knot_count > LC_MPFI_MAX_KNOTS_V1) {
        return reject(&input, LC_MPFI_WIRE_RESOURCE_LIMIT);
    }
    if (knot_count > SIZE_MAX / 64
        || available(&input) != (size_t) knot_count * 64) {
        return reject(&input, LC_MPFI_WIRE_NONCANONICAL);
    }
    if (memcmp(fields[0].bytes, definition_domain, sizeof(definition_domain) - 1) != 0) {
        return reject(&input, LC_MPFI_WIRE_UNKNOWN_RELEASE);
    }
    for (size_t index = 1; index <= 17; ++index) {
        bool required = index == 1 || index == 2 || index == 3 || index == 4
            || index == 5 || index == 6 || index == 8 || index == 9
            || index == 14 || index == 15 || index == 17;

        if (required && !fixed_one(fields[index])) {
            return reject(&input, LC_MPFI_WIRE_UNKNOWN_RELEASE);
        }
    }
    if (memcmp(fields[7].bytes, "\x01\x01\x01\x01", 4) != 0
        || memcmp(fields[10].bytes, "\x01\x01\x01\x01", 4) != 0
        || fields[13].bytes[0] < 1 || fields[13].bytes[0] > 3
        || memcmp(fields[16].bytes, formula_release_v1, 32) != 0) {
        return reject(&input, LC_MPFI_WIRE_UNKNOWN_RELEASE);
    }
    bool adapting_ok = field_rational(fields[11], adapting);
    bool background_ok = field_rational(fields[12], background);

    if (!adapting_ok || !background_ok
        || mpq_sgn(adapting) <= 0
        || mpq_sgn(background) <= 0
        || mpq_cmp_ui(background, 1, 1) > 0) {
        if (adapting_ok) {
            mpq_clear(adapting);
        }
        if (background_ok) {
            mpq_clear(background);
        }
        return reject(&input, LC_MPFI_WIRE_NONCANONICAL);
    }
    mpq_clear(adapting);
    mpq_clear(background);

    bool metric_aa_ok = field_rational(fields[18], metric_aa);
    bool metric_ab_ok = field_rational(fields[19], metric_ab);
    bool metric_bb_ok = field_rational(fields[20], metric_bb);

    if (!metric_aa_ok || !metric_ab_ok || !metric_bb_ok) {
        if (metric_aa_ok) {
            mpq_clear(metric_aa);
        }
        if (metric_ab_ok) {
            mpq_clear(metric_ab);
        }
        if (metric_bb_ok) {
            mpq_clear(metric_bb);
        }
        return reject(&input, LC_MPFI_WIRE_NONCANONICAL);
    }
    mpq_init(determinant);
    mpq_init(product);
    mpq_mul(determinant, metric_aa, metric_bb);
    mpq_mul(product, metric_ab, metric_ab);
    mpq_sub(determinant, determinant, product);
    if (mpq_sgn(metric_aa) <= 0 || mpq_sgn(determinant) <= 0) {
        mpq_clear(product);
        mpq_clear(determinant);
        mpq_clear(metric_bb);
        mpq_clear(metric_ab);
        mpq_clear(metric_aa);
        return reject(&input, LC_MPFI_WIRE_NONCANONICAL);
    }
    mpq_clear(product);
    mpq_clear(determinant);

    mpfi_init2(&job->context[0], precision);
    mpfi_init2(&job->context[1], precision);
    job->context_ready = true;
    if (!field_to_interval(fields[11], &job->context[0])
        || !field_to_interval(fields[12], &job->context[1])) {
        return reject(&input, LC_MPFI_WIRE_NONCANONICAL);
    }
    job->surround = fields[13].bytes[0];
    memcpy(job->formula_release, fields[16].bytes, 32);
    if (!lc_mpfi_region_init(&job->region, (size_t) knot_count, precision)) {
        return reject(&input, LC_MPFI_WIRE_ALLOCATION_FAILED);
    }
    job->region_ready = true;
    if (!field_to_interval(fields[18], &job->region.metric_aa)
        || !field_to_interval(fields[19], &job->region.metric_ab)
        || !field_to_interval(fields[20], &job->region.metric_bb)) {
        return reject(&input, LC_MPFI_WIRE_NONCANONICAL);
    }
    bool have_previous_tone = false;
    mpq_t previous_tone;

    for (size_t index = 0; index < (size_t) knot_count; ++index) {
        lc_mpfi_slice knot_fields[4];
        lc_mpfi_region_knot *target = job->region.knots + index;
        mpq_t tone;
        mpq_t radius;
        mpq_t center_a;
        mpq_t center_b;
        bool tone_ok;
        bool radius_ok;
        bool center_a_ok;
        bool center_b_ok;

        for (size_t coordinate = 0; coordinate < 4; ++coordinate) {
            if (!read_blob(&input, 8, knot_fields + coordinate)) {
                if (have_previous_tone) {
                    mpq_clear(previous_tone);
                }
                return false;
            }
        }
        tone_ok = field_rational(knot_fields[0], tone);
        radius_ok = field_rational(knot_fields[3], radius);
        center_a_ok = field_rational(knot_fields[1], center_a);
        center_b_ok = field_rational(knot_fields[2], center_b);
        if (!tone_ok || !radius_ok || !center_a_ok || !center_b_ok
            || mpq_sgn(radius) < 0
            || (have_previous_tone && mpq_cmp(tone, previous_tone) <= 0)) {
            if (tone_ok) {
                mpq_clear(tone);
            }
            if (radius_ok) {
                mpq_clear(radius);
            }
            if (center_a_ok) {
                mpq_clear(center_a);
            }
            if (center_b_ok) {
                mpq_clear(center_b);
            }
            if (have_previous_tone) {
                mpq_clear(previous_tone);
            }
            return reject(&input, LC_MPFI_WIRE_NONCANONICAL);
        }
        if (have_previous_tone) {
            mpq_clear(previous_tone);
        }
        mpq_init(previous_tone);
        mpq_set(previous_tone, tone);
        have_previous_tone = true;
        mpq_clear(radius);
        mpq_clear(tone);
        mpq_clear(center_a);
        mpq_clear(center_b);
        if (!field_to_interval(knot_fields[0], &target->tone)
            || !field_to_interval(knot_fields[1], &target->center_a)
            || !field_to_interval(knot_fields[2], &target->center_b)
            || !field_to_interval(knot_fields[3], &target->radius_squared)) {
            if (have_previous_tone) {
                mpq_clear(previous_tone);
            }
            return reject(&input, LC_MPFI_WIRE_NONCANONICAL);
        }
    }
    if (have_previous_tone) {
        mpq_clear(previous_tone);
    }
    return finish(&input);
}

static bool
parse_domain(
    lc_mpfi_domain *domain,
    lc_mpfi_slice encoded,
    const uint8_t expected[32],
    lc_mpfi_wire_error *error
)
{
    mpfi_reader input = {encoded.bytes, encoded.length, 0, error};
    uint8_t release;
    uint64_t range_count;
    uint64_t maximum;
    uint64_t total = 0;

    if (!expect(&input, domain_magic, sizeof(domain_magic), LC_MPFI_WIRE_BAD_MAGIC)
        || !read_u8(&input, &release)
        || release != 1
        || !read_u64(&input, &domain->point_count)
        || domain->point_count == 0
        || domain->point_count > UINT64_C(0x1000000)
        || !read_u64(&input, &range_count)) {
        return *input.error != LC_MPFI_WIRE_OK
            ? false
            : reject(&input, LC_MPFI_WIRE_NONCANONICAL);
    }
    maximum = domain->point_count;
    if (UINT64_C(0x1000001) - domain->point_count < maximum) {
        maximum = UINT64_C(0x1000001) - domain->point_count;
    }
    if (range_count == 0 || range_count > maximum
        || range_count > SIZE_MAX / sizeof(*domain->ranges)
        || range_count > available(&input) / 8
        || (size_t) range_count * 8 != available(&input)) {
        return reject(&input, LC_MPFI_WIRE_LENGTH_OUT_OF_BOUNDS);
    }
    domain->ranges = calloc((size_t) range_count, sizeof(*domain->ranges));
    if (domain->ranges == NULL) {
        return reject(&input, LC_MPFI_WIRE_ALLOCATION_FAILED);
    }
    domain->range_count = (size_t) range_count;
    for (size_t index = 0; index < domain->range_count; ++index) {
        lc_mpfi_ordinal_range *range = domain->ranges + index;

        if (!read_u32(&input, &range->start) || !read_u32(&input, &range->end)) {
            return false;
        }
        if (range->start >= range->end || range->end > UINT32_C(0x1000000)
            || (index != 0 && range->start <= domain->ranges[index - 1].end)) {
            return reject(&input, LC_MPFI_WIRE_NONCANONICAL);
        }
        total += (uint64_t) range->end - range->start;
    }
    if (total != domain->point_count || !finish(&input)) {
        return *input.error != LC_MPFI_WIRE_OK
            ? false
            : reject(&input, LC_MPFI_WIRE_NONCANONICAL);
    }
    content_identity(
        domain_identity_label,
        sizeof(domain_identity_label) - 1,
        encoded.bytes,
        encoded.length,
        domain->identity
    );
    return memcmp(domain->identity, expected, 32) == 0
        || reject(&input, LC_MPFI_WIRE_DIGEST_MISMATCH);
}

static bool
parse_policy(
    lc_mpfi_policy *policy,
    lc_mpfi_slice encoded,
    const uint8_t expected[32],
    lc_mpfi_wire_error *error
)
{
    mpfi_reader input = {encoded.bytes, encoded.length, 0, error};
    uint8_t equality_release;
    uint8_t comparator_count;

    if (!expect(&input, policy_magic, sizeof(policy_magic), LC_MPFI_WIRE_BAD_MAGIC)
        || !read_u8(&input, &equality_release)
        || !read_u8(&input, &comparator_count)) {
        return false;
    }
    if (equality_release != 1 || comparator_count != 2) {
        return reject(&input, LC_MPFI_WIRE_UNKNOWN_RELEASE);
    }
    for (uint8_t expected_kind = 1; expected_kind <= 2; ++expected_kind) {
        uint8_t kind;
        uint32_t rung_count;
        uint32_t previous = 0;
        size_t minimum_tail = expected_kind == 1 ? 41 : 16;
        uint32_t *ladder = NULL;

        if (!read_u8(&input, &kind) || !read_u32(&input, &rung_count)) {
            return false;
        }
        if (kind != expected_kind || rung_count == 0
            || available(&input) < minimum_tail
            || rung_count > (available(&input) - minimum_tail) / 4) {
            return reject(&input, LC_MPFI_WIRE_NONCANONICAL);
        }
        if (rung_count > LC_MPFI_MAX_POLICY_RUNGS_V1) {
            return reject(&input, LC_MPFI_WIRE_RESOURCE_LIMIT);
        }
        if (expected_kind == 2) {
            if ((size_t) rung_count > SIZE_MAX / sizeof(*policy->precision_ladder)) {
                return reject(&input, LC_MPFI_WIRE_LENGTH_OUT_OF_BOUNDS);
            }
            ladder = calloc(rung_count, sizeof(*ladder));
            if (ladder == NULL) {
                return reject(&input, LC_MPFI_WIRE_ALLOCATION_FAILED);
            }
        }
        for (size_t index = 0; index < rung_count; ++index) {
            uint32_t precision;

            if (!read_u32(&input, &precision)) {
                free(ladder);
                return false;
            }
            if (precision == 0 || (index != 0 && precision <= previous)
                || (uint64_t) precision > (uint64_t) MPFR_PREC_MAX) {
                free(ladder);
                return reject(&input, LC_MPFI_WIRE_NONCANONICAL);
            }
            if (precision > LC_MPFI_MAX_PRECISION_BITS_V1) {
                free(ladder);
                return reject(&input, LC_MPFI_WIRE_RESOURCE_LIMIT);
            }
            if (ladder != NULL) {
                ladder[index] = precision;
            }
            previous = precision;
        }
        if (expected_kind == 1) {
            uint64_t ignored;

            if (!read_u64(&input, &ignored) || !read_u64(&input, &ignored)) {
                free(ladder);
                return false;
            }
        } else {
            if (!read_u64(&input, &policy->per_point_work)
                || !read_u64(&input, &policy->global_pregrant)) {
                free(ladder);
                return false;
            }
            policy->precision_ladder = ladder;
            policy->precision_count = rung_count;
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
        || reject(&input, LC_MPFI_WIRE_DIGEST_MISMATCH);
}

static bool
formula_matches(lc_mpfi_slice formula)
{
    lc_mpfi_sha256 state;
    uint8_t encoded_length[8];
    uint8_t digest[32];

    if (formula.length != formula_spec_length) {
        return false;
    }
    lc_mpfi_write_u64_be(encoded_length, (uint64_t) formula.length);
    lc_mpfi_sha256_init(&state);
    lc_mpfi_sha256_update(&state, formula_domain, sizeof(formula_domain) - 1);
    lc_mpfi_sha256_update(&state, encoded_length, sizeof(encoded_length));
    lc_mpfi_sha256_update(&state, formula.bytes, formula.length);
    lc_mpfi_sha256_finish(&state, digest);
    return memcmp(digest, formula_release_v1, sizeof(formula_release_v1)) == 0;
}

bool
lc_mpfi_parse_job(
    lc_mpfi_job *job,
    const uint8_t *bytes,
    size_t length,
    lc_mpfi_wire_error *error
)
{
    mpfi_reader input;
    lc_mpfi_slice definition;
    lc_mpfi_slice formula;
    lc_mpfi_slice domain;
    lc_mpfi_slice policy;
    lc_mpfi_slice definition_digest;
    lc_mpfi_slice declared_formula;
    lc_mpfi_slice domain_identity;
    lc_mpfi_slice policy_identity;
    uint8_t actual[32];

    memset(job, 0, sizeof(*job));
    *error = LC_MPFI_WIRE_OK;
    if (length > (size_t) LC_MPFI_MAX_JOB_BYTES_V1) {
        *error = LC_MPFI_WIRE_RESOURCE_LIMIT;
        return false;
    }
    input = (mpfi_reader) {bytes, length, 0, error};
    if (!expect(&input, job_magic, sizeof(job_magic), LC_MPFI_WIRE_BAD_MAGIC)
        || !take(&input, 32, &definition_digest)
        || !read_blob(&input, SIZE_MAX, &definition)
        || !take(&input, 32, &declared_formula)
        || !read_blob(&input, formula_spec_length, &formula)
        || !take(&input, 32, &domain_identity)
        || !read_blob(&input, SIZE_MAX, &domain)
        || !take(&input, 32, &policy_identity)
        || !read_blob(&input, SIZE_MAX, &policy)
        || !finish(&input)) {
        lc_mpfi_job_clear(job);
        return false;
    }
    lc_mpfi_sha256_bytes(definition.bytes, definition.length, actual);
    if (memcmp(actual, definition_digest.bytes, 32) != 0) {
        *error = LC_MPFI_WIRE_DIGEST_MISMATCH;
        lc_mpfi_job_clear(job);
        return false;
    }
    if (!parse_policy(&job->policy, policy, policy_identity.bytes, error)
        || job->policy.precision_count == 0
        || !parse_domain(&job->domain, domain, domain_identity.bytes, error)) {
        lc_mpfi_job_clear(job);
        return false;
    }
    job->maximum_precision = job->policy.precision_ladder[
        job->policy.precision_count - 1
    ];
    if (!parse_definition(job, definition, job->maximum_precision, error)
        || memcmp(declared_formula.bytes, job->formula_release, 32) != 0
        || !formula_matches(formula)
        || memcmp(declared_formula.bytes, formula_release_v1, 32) != 0) {
        if (*error == LC_MPFI_WIRE_OK) {
            *error = LC_MPFI_WIRE_DIGEST_MISMATCH;
        }
        lc_mpfi_job_clear(job);
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
lc_mpfi_job_clear(lc_mpfi_job *job)
{
    free(job->policy.precision_ladder);
    free(job->domain.ranges);
    if (job->region_ready) {
        lc_mpfi_region_clear(&job->region);
    }
    if (job->context_ready) {
        mpfi_clear(&job->context[1]);
        mpfi_clear(&job->context[0]);
    }
    memset(job, 0, sizeof(*job));
}

void
lc_mpfi_domain_iterator_init(
    lc_mpfi_domain_iterator *iterator,
    const lc_mpfi_domain *domain
)
{
    iterator->domain = domain;
    iterator->range_index = 0;
    iterator->ordinal = domain->ranges[0].start;
    iterator->emitted = 0;
}

bool
lc_mpfi_domain_iterator_next(
    lc_mpfi_domain_iterator *iterator,
    uint32_t *ordinal
)
{
    if (iterator->emitted == iterator->domain->point_count) {
        return false;
    }
    *ordinal = iterator->ordinal;
    ++iterator->emitted;
    ++iterator->ordinal;
    if (iterator->ordinal == iterator->domain->ranges[iterator->range_index].end
        && iterator->emitted != iterator->domain->point_count) {
        ++iterator->range_index;
        iterator->ordinal = iterator->domain->ranges[iterator->range_index].start;
    }
    return true;
}

void
lc_mpfi_ordinal_to_rgb(uint32_t ordinal, uint8_t rgb[3])
{
    rgb[0] = (uint8_t) (ordinal >> 16);
    rgb[1] = (uint8_t) (ordinal >> 8);
    rgb[2] = (uint8_t) ordinal;
}

const char *
lc_mpfi_wire_error_name(lc_mpfi_wire_error error)
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
        "resource_limit",
    };

    return (unsigned) error < sizeof(names) / sizeof(names[0])
        ? names[error]
        : "unknown_wire_error";
}

bool
lc_mpfi_write_all(int descriptor, const uint8_t *bytes, size_t length)
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
