use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;
use std::sync::OnceLock;

use proptest::prelude::*;

use crate::Srgb8;
use crate::lcs_occurrence::{
    AdaptingLuminanceCdM2, AppearanceContextId, AppearanceContextSchemaReleaseId, AppearanceState,
    BackgroundLuminanceRatio, ColorSignal, IEC_SRGB_D65_XYZ_FRAME_V1, LcsOccurrence,
    SurroundProfileId, derive_modeled_tristimulus_v1,
};
use crate::spaces::srgb::decode_8bit;
use crate::spaces::{cam16, cat16, srgb, vc};

const FORMULA_BYTES: &[u8] = include_bytes!("../contracts/contextual-region-formula-v1.lcir");

const TYPE_DECLARATIONS: [&str; 4] = [
    "type u8 unsigned_integer_0_255",
    "type real mathematical_real",
    "type bool exact_boolean",
    "type surround_profile closed_enum",
];

const OPERATOR_DECLARATIONS: [&str; 20] = [
    "operator lookup 2 real table_u8_exact_dyadic_at_ordinal",
    "operator eq 2 bool exact_same_type_equality",
    "operator select 3 same bool_true_second_else_third",
    "operator add 2 real exact_x_plus_y",
    "operator sub 2 real exact_x_minus_y",
    "operator mul 2 real exact_x_times_y",
    "operator div 2 real domain_y_ne_zero_x_div_y_else_domain_unproven",
    "operator min 2 real exact_lesser_real",
    "operator max 2 real exact_greater_real",
    "operator root3 1 real domain_x_ge_zero_unique_y_ge_zero_y_cubed_eq_x_else_domain_unproven",
    "operator sqrt 1 real domain_x_ge_zero_unique_y_ge_zero_y_squared_eq_x_else_domain_unproven",
    "operator exp 1 real analytic_natural_exponential",
    "operator log 1 real domain_x_gt_zero_analytic_natural_logarithm_else_domain_unproven",
    "operator sin 1 real analytic_sine_radians",
    "operator cos 1 real analytic_cosine_radians",
    "operator abs 1 real exact_absolute_value",
    "operator sign 1 real negative_minus_one_zero_zero_positive_one",
    "operator pow_pos 2 real domain_x_gt_zero_exp_y_mul_log_x_else_domain_unproven",
    "operator pow_nn 2 real if_x_eq_zero_and_y_gt_zero_zero_else_pow_pos",
    "operator ratio0 2 real if_x_eq_zero_and_y_eq_zero_zero_else_domain_y_gt_zero_x_div_y",
];

const DRIVER_RULES: [&str; 6] = [
    "rule tone_domain closed_first_last",
    "rule out_of_tone_domain outside",
    "rule one_knot_tone exact_equality_required",
    "rule one_knot_predicate singleton_f_le_zero",
    "rule multi_knot_predicate piecewise_linear_segment_f_le_zero",
    "rule boundary inclusive",
];

const POINT_CHECKPOINTS: [(&str, &str); 39] = [
    ("decoded_r", "linear_r"),
    ("decoded_g", "linear_g"),
    ("decoded_b", "linear_b"),
    ("xyz_x", "xyz_x"),
    ("xyz_y", "xyz_y"),
    ("xyz_z", "xyz_z"),
    ("surround_f", "surround_f"),
    ("surround_c", "surround_c"),
    ("surround_nc", "surround_nc"),
    ("n", "n"),
    ("fl", "fl"),
    ("nbb", "nbb"),
    ("ncb", "nbb"),
    ("z", "z"),
    ("d", "d"),
    ("rgb_d_l", "rgb_d_l"),
    ("rgb_d_m", "rgb_d_m"),
    ("rgb_d_s", "rgb_d_s"),
    ("aw", "aw"),
    ("t_inner", "t_inner"),
    ("fl_pow_025", "fl_pow_025"),
    ("lms_l", "lms_l"),
    ("lms_m", "lms_m"),
    ("lms_s", "lms_s"),
    ("response_l", "response_l"),
    ("response_m", "response_m"),
    ("response_s", "response_s"),
    ("opponent_a", "opponent_a"),
    ("opponent_b", "opponent_b"),
    ("opponent_norm", "opponent_norm"),
    ("achrom", "achrom"),
    ("j", "j"),
    ("e_hue_times_norm", "e_hue_times_norm"),
    ("t", "t"),
    ("m", "m"),
    ("jp", "jp"),
    ("mp", "mp"),
    ("ap", "ap"),
    ("bp", "bp"),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ValueType {
    U8,
    Real,
    Bool,
    Surround,
    DecodeTable,
}

impl ValueType {
    fn parse(token: &str) -> Result<Self, String> {
        match token {
            "u8" => Ok(Self::U8),
            "real" => Ok(Self::Real),
            "bool" => Ok(Self::Bool),
            "surround_profile" => Ok(Self::Surround),
            _ => Err(format!("unknown value type {token}")),
        }
    }
}

#[derive(Clone, Debug)]
struct Node {
    name: String,
    result: ValueType,
    operator: String,
    arguments: Vec<String>,
}

#[derive(Clone, Debug)]
struct Program {
    inputs: Vec<(String, ValueType)>,
    nodes: Vec<Node>,
    checkpoints: Vec<(String, String)>,
    outputs: Vec<(String, ValueType)>,
}

#[derive(Clone, Debug)]
struct Formula {
    decode: [u64; 256],
    literal_order: Vec<(String, u64)>,
    literals: BTreeMap<String, u64>,
    enum_values: BTreeMap<String, u8>,
    point: Program,
    segment: Program,
    singleton: Program,
}

struct Lines<'a> {
    values: Vec<&'a str>,
    cursor: usize,
}

impl<'a> Lines<'a> {
    fn next(&mut self) -> Result<&'a str, String> {
        let index = self.cursor;
        let value = self
            .values
            .get(index)
            .copied()
            .ok_or_else(|| format!("unexpected end at line {}", index + 1))?;
        self.cursor += 1;
        Ok(value)
    }

    fn expect(&mut self, expected: &str) -> Result<(), String> {
        let actual = self.next()?;
        if actual == expected {
            Ok(())
        } else {
            Err(format!(
                "line {}: expected {expected:?}, got {actual:?}",
                self.cursor
            ))
        }
    }
}

fn parse_formula(bytes: &[u8]) -> Result<Formula, String> {
    if !bytes.is_ascii() {
        return Err("formula is not ASCII".into());
    }
    if !bytes.ends_with(b"\n") || bytes.ends_with(b"\n\n") {
        return Err("formula must have exactly one final LF".into());
    }
    if bytes.iter().any(|byte| matches!(byte, b'\r' | b'\t' | 0)) {
        return Err("formula contains a forbidden control byte".into());
    }
    let text = std::str::from_utf8(bytes).map_err(|error| error.to_string())?;
    let raw_lines = text
        .strip_suffix('\n')
        .unwrap()
        .split('\n')
        .collect::<Vec<_>>();
    for (index, line) in raw_lines.iter().enumerate() {
        if line.is_empty()
            || line.starts_with(' ')
            || line.ends_with(' ')
            || line.contains("  ")
            || line.contains('#')
        {
            return Err(format!("line {} is not canonical", index + 1));
        }
        if line
            .split(' ')
            .any(|token| token.is_empty() || !token.bytes().all(is_token_byte))
        {
            return Err(format!("line {} contains an invalid token", index + 1));
        }
    }

    let mut lines = Lines {
        values: raw_lines.clone(),
        cursor: 0,
    };
    lines.expect("labcolors_exact_real_ssa 1")?;
    lines.expect("arithmetic exact_real_v1")?;
    lines.expect("types 4")?;
    for declaration in TYPE_DECLARATIONS {
        lines.expect(declaration)?;
    }
    lines.expect("operators 20")?;
    for declaration in OPERATOR_DECLARATIONS {
        lines.expect(declaration)?;
    }

    lines.expect("decode_table decode_srgb8 256")?;
    let mut decode = [0_u64; 256];
    for (ordinal, slot) in decode.iter_mut().enumerate() {
        let fields = fields(lines.next()?, 3)?;
        require(fields[0] == "decode", "invalid decode record")?;
        require(
            fields[1] == format!("{ordinal:02x}"),
            "decode ordinals are not canonical",
        )?;
        *slot = parse_finite_bits(fields[2])?;
    }

    lines.expect("literals 56")?;
    let mut literals = BTreeMap::new();
    let mut literal_order = Vec::with_capacity(56);
    let mut literal_payloads = BTreeSet::new();
    for _ in 0..56 {
        let values = fields(lines.next()?, 3)?;
        require(values[0] == "literal", "invalid literal record")?;
        require(is_identifier(values[1]), "invalid literal name")?;
        let bits = parse_finite_bits(values[2])?;
        require(
            literals.insert(values[1].to_owned(), bits).is_none(),
            "duplicate literal name",
        )?;
        require(literal_payloads.insert(bits), "duplicate literal payload")?;
        literal_order.push((values[1].to_owned(), bits));
    }

    lines.expect("enum_type surround_profile 3")?;
    let mut enum_values = BTreeMap::new();
    for expected in [
        ("surround_average", "01"),
        ("surround_dim", "02"),
        ("surround_dark", "03"),
    ] {
        let values = fields(lines.next()?, 4)?;
        require(
            values == ["enum", "surround_profile", expected.0, expected.1],
            "surround enum is not the closed registered set",
        )?;
        let byte = u8::from_str_radix(values[3], 16).map_err(|error| error.to_string())?;
        require(
            enum_values.insert(values[2].to_owned(), byte).is_none(),
            "duplicate enum value",
        )?;
    }

    let mut global_names = BTreeSet::from(["decode_srgb8"]);
    for name in literals.keys().chain(enum_values.keys()) {
        require(
            global_names.insert(name.as_str()),
            "duplicate or shadowed global symbol",
        )?;
    }

    let globals = global_symbols(&literals, &enum_values);
    let point = parse_program(
        &mut lines,
        "point",
        6,
        226,
        Some(39),
        3,
        &[
            ("r8", ValueType::U8),
            ("g8", ValueType::U8),
            ("b8", ValueType::U8),
            ("adapting_luminance", ValueType::Real),
            ("background_ratio", ValueType::Real),
            ("surround", ValueType::Surround),
        ],
        &["jp", "ap", "bp"],
        &globals,
    )?;
    require(
        point
            .checkpoints
            .iter()
            .map(|(name, target)| (name.as_str(), target.as_str()))
            .eq(POINT_CHECKPOINTS),
        "point checkpoints are not the registered diagnostic ABI",
    )?;
    let segment = parse_program(
        &mut lines,
        "segment",
        14,
        27,
        None,
        1,
        &[
            ("segment_t", ValueType::Real),
            ("segment_a", ValueType::Real),
            ("segment_b", ValueType::Real),
            ("segment_t0", ValueType::Real),
            ("segment_t1", ValueType::Real),
            ("segment_c0a", ValueType::Real),
            ("segment_c0b", ValueType::Real),
            ("segment_c1a", ValueType::Real),
            ("segment_c1b", ValueType::Real),
            ("segment_rho0", ValueType::Real),
            ("segment_rho1", ValueType::Real),
            ("segment_g00", ValueType::Real),
            ("segment_g01", ValueType::Real),
            ("segment_g11", ValueType::Real),
        ],
        &["segment_f"],
        &globals,
    )?;
    let singleton = parse_program(
        &mut lines,
        "singleton",
        8,
        12,
        None,
        1,
        &[
            ("singleton_a", ValueType::Real),
            ("singleton_b", ValueType::Real),
            ("singleton_ca", ValueType::Real),
            ("singleton_cb", ValueType::Real),
            ("singleton_rho", ValueType::Real),
            ("singleton_g00", ValueType::Real),
            ("singleton_g01", ValueType::Real),
            ("singleton_g11", ValueType::Real),
        ],
        &["singleton_f"],
        &globals,
    )?;
    lines.expect("driver 6")?;
    for rule in DRIVER_RULES {
        lines.expect(rule)?;
    }
    lines.expect("end")?;
    require(
        lines.cursor == lines.values.len(),
        "trailing formula records",
    )?;

    validate_reachability(&point, &segment, &singleton, &literals)?;
    let formula = Formula {
        decode,
        literal_order,
        literals,
        enum_values,
        point,
        segment,
        singleton,
    };
    require(
        emit_formula(&formula).as_bytes() == bytes,
        "parse/emit is not byte-identical",
    )?;
    Ok(formula)
}

fn registered_formula() -> &'static Formula {
    static FORMULA: OnceLock<Formula> = OnceLock::new();
    FORMULA.get_or_init(|| parse_formula(FORMULA_BYTES).unwrap())
}

fn emit_formula(formula: &Formula) -> String {
    let mut output = String::new();
    writeln!(output, "labcolors_exact_real_ssa 1").unwrap();
    writeln!(output, "arithmetic exact_real_v1").unwrap();
    writeln!(output, "types {}", TYPE_DECLARATIONS.len()).unwrap();
    for declaration in TYPE_DECLARATIONS {
        writeln!(output, "{declaration}").unwrap();
    }
    writeln!(output, "operators {}", OPERATOR_DECLARATIONS.len()).unwrap();
    for declaration in OPERATOR_DECLARATIONS {
        writeln!(output, "{declaration}").unwrap();
    }
    writeln!(output, "decode_table decode_srgb8 {}", formula.decode.len()).unwrap();
    for (ordinal, bits) in formula.decode.iter().enumerate() {
        writeln!(output, "decode {ordinal:02x} {bits:016x}").unwrap();
    }
    writeln!(output, "literals {}", formula.literal_order.len()).unwrap();
    for (name, bits) in &formula.literal_order {
        writeln!(output, "literal {name} {bits:016x}").unwrap();
    }
    writeln!(
        output,
        "enum_type surround_profile {}",
        formula.enum_values.len()
    )
    .unwrap();
    for name in ["surround_average", "surround_dim", "surround_dark"] {
        writeln!(
            output,
            "enum surround_profile {name} {:02x}",
            formula.enum_values[name],
        )
        .unwrap();
    }
    emit_program(&mut output, "point", &formula.point, true);
    emit_program(&mut output, "segment", &formula.segment, false);
    emit_program(&mut output, "singleton", &formula.singleton, false);
    writeln!(output, "driver {}", DRIVER_RULES.len()).unwrap();
    for rule in DRIVER_RULES {
        writeln!(output, "{rule}").unwrap();
    }
    writeln!(output, "end").unwrap();
    output
}

fn emit_program(output: &mut String, prefix: &str, program: &Program, checkpoints: bool) {
    writeln!(output, "{prefix}_inputs {}", program.inputs.len()).unwrap();
    for (name, value_type) in &program.inputs {
        writeln!(output, "input {name} {}", value_type_name(*value_type)).unwrap();
    }
    writeln!(output, "{prefix}_nodes {}", program.nodes.len()).unwrap();
    for node in &program.nodes {
        writeln!(
            output,
            "node {} {} {} {}",
            node.name,
            value_type_name(node.result),
            node.operator,
            node.arguments.join(" "),
        )
        .unwrap();
    }
    if checkpoints {
        writeln!(output, "{prefix}_checkpoints {}", program.checkpoints.len(),).unwrap();
        for (name, target) in &program.checkpoints {
            writeln!(output, "checkpoint {name} {target}").unwrap();
        }
    }
    writeln!(output, "{prefix}_outputs {}", program.outputs.len()).unwrap();
    for (name, value_type) in &program.outputs {
        writeln!(output, "output {name} {}", value_type_name(*value_type)).unwrap();
    }
}

fn value_type_name(value_type: ValueType) -> &'static str {
    match value_type {
        ValueType::U8 => "u8",
        ValueType::Real => "real",
        ValueType::Bool => "bool",
        ValueType::Surround => "surround_profile",
        ValueType::DecodeTable => panic!("decode table is not a serializable value type"),
    }
}

#[allow(clippy::too_many_arguments)]
fn parse_program(
    lines: &mut Lines<'_>,
    prefix: &str,
    input_count: usize,
    node_count: usize,
    checkpoint_count: Option<usize>,
    output_count: usize,
    expected_inputs: &[(&str, ValueType)],
    expected_outputs: &[&str],
    globals: &BTreeMap<String, ValueType>,
) -> Result<Program, String> {
    lines.expect(&format!("{prefix}_inputs {input_count}"))?;
    let mut symbols = globals.clone();
    let mut inputs = Vec::new();
    for expected in expected_inputs {
        let values = fields(lines.next()?, 3)?;
        require(values[0] == "input", "invalid input record")?;
        let result = ValueType::parse(values[2])?;
        require(
            values[1] == expected.0 && result == expected.1,
            "program inputs are not the registered interface",
        )?;
        insert_symbol(&mut symbols, values[1], result)?;
        inputs.push((values[1].to_owned(), result));
    }

    lines.expect(&format!("{prefix}_nodes {node_count}"))?;
    let mut nodes = Vec::new();
    for _ in 0..node_count {
        let values = lines.next()?.split(' ').collect::<Vec<_>>();
        require(
            values.len() >= 5 && values[0] == "node",
            "invalid node record",
        )?;
        require(is_identifier(values[1]), "invalid node name")?;
        let result = ValueType::parse(values[2])?;
        let arguments = values[4..]
            .iter()
            .map(|value| (*value).to_owned())
            .collect::<Vec<_>>();
        validate_operation(values[3], result, &arguments, &symbols)?;
        insert_symbol(&mut symbols, values[1], result)?;
        nodes.push(Node {
            name: values[1].to_owned(),
            result,
            operator: values[3].to_owned(),
            arguments,
        });
    }

    let mut checkpoints = Vec::new();
    if let Some(count) = checkpoint_count {
        lines.expect(&format!("{prefix}_checkpoints {count}"))?;
        let mut names = BTreeSet::new();
        let node_names = nodes
            .iter()
            .map(|node| node.name.as_str())
            .collect::<BTreeSet<_>>();
        for _ in 0..count {
            let values = fields(lines.next()?, 3)?;
            require(values[0] == "checkpoint", "invalid checkpoint record")?;
            require(names.insert(values[1]), "duplicate checkpoint name")?;
            require(
                node_names.contains(values[2]) && symbols.get(values[2]) == Some(&ValueType::Real),
                "checkpoint must reference a preceding real node",
            )?;
            checkpoints.push((values[1].to_owned(), values[2].to_owned()));
        }
    }

    lines.expect(&format!("{prefix}_outputs {output_count}"))?;
    let mut outputs = Vec::new();
    for expected in expected_outputs {
        let values = fields(lines.next()?, 3)?;
        require(
            values[0] == "output" && values[1] == *expected,
            "program outputs are not the registered interface",
        )?;
        let result = ValueType::parse(values[2])?;
        require(
            result == ValueType::Real && symbols.get(values[1]) == Some(&result),
            "output must name a preceding real node",
        )?;
        outputs.push((values[1].to_owned(), result));
    }
    Ok(Program {
        inputs,
        nodes,
        checkpoints,
        outputs,
    })
}

fn validate_operation(
    operator: &str,
    result: ValueType,
    arguments: &[String],
    symbols: &BTreeMap<String, ValueType>,
) -> Result<(), String> {
    let types = arguments
        .iter()
        .map(|name| {
            symbols
                .get(name)
                .copied()
                .ok_or_else(|| format!("unknown or forward reference {name}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    match operator {
        "lookup" => require(
            result == ValueType::Real
                && types.as_slice() == [ValueType::DecodeTable, ValueType::U8],
            "lookup type/arity mismatch",
        ),
        "eq" => require(
            result == ValueType::Bool
                && types.len() == 2
                && types[0] == types[1]
                && types[0] != ValueType::DecodeTable,
            "eq type/arity mismatch",
        ),
        "select" => require(
            types.len() == 3
                && types[0] == ValueType::Bool
                && types[1] == types[2]
                && types[1] != ValueType::DecodeTable
                && result == types[1],
            "select type/arity mismatch",
        ),
        "root3" | "sqrt" | "exp" | "log" | "sin" | "cos" | "abs" | "sign" => require(
            result == ValueType::Real && types.as_slice() == [ValueType::Real],
            "unary real type/arity mismatch",
        ),
        "add" | "sub" | "mul" | "div" | "min" | "max" | "pow_pos" | "pow_nn" | "ratio0" => require(
            result == ValueType::Real && types.as_slice() == [ValueType::Real, ValueType::Real],
            "binary real type/arity mismatch",
        ),
        _ => Err(format!("unknown operator {operator}")),
    }
}

fn validate_reachability(
    point: &Program,
    segment: &Program,
    singleton: &Program,
    literals: &BTreeMap<String, u64>,
) -> Result<(), String> {
    let mut used_globals = BTreeSet::new();
    let mut used_operators = BTreeSet::new();
    for program in [point, segment, singleton] {
        let mut program_symbols = BTreeSet::new();
        let nodes = program
            .nodes
            .iter()
            .map(|node| (node.name.as_str(), node))
            .collect::<BTreeMap<_, _>>();
        let mut roots = program
            .outputs
            .iter()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>();
        let mut reachable = BTreeSet::new();
        while let Some(name) = roots.pop() {
            if let Some(node) = nodes.get(name) {
                if !reachable.insert(name) {
                    continue;
                }
                used_operators.insert(node.operator.as_str());
                roots.extend(node.arguments.iter().map(String::as_str));
            } else {
                used_globals.insert(name);
                program_symbols.insert(name);
            }
        }
        require(
            reachable.len() == program.nodes.len(),
            "program contains an unreachable node",
        )?;
        for (input, _) in &program.inputs {
            require(
                program_symbols.contains(input.as_str()),
                "program contains an unused input",
            )?;
        }
    }
    for literal in literals.keys() {
        require(
            used_globals.contains(literal.as_str()),
            "formula contains an unused literal",
        )?;
    }
    require(
        used_globals.contains("decode_srgb8"),
        "formula contains an unused decode table",
    )?;
    let declared_operators = OPERATOR_DECLARATIONS
        .iter()
        .map(|declaration| declaration.split(' ').nth(1).unwrap())
        .collect::<BTreeSet<_>>();
    require(
        used_operators == declared_operators,
        "formula contains an unused operator declaration",
    )
}

fn global_symbols(
    literals: &BTreeMap<String, u64>,
    enum_values: &BTreeMap<String, u8>,
) -> BTreeMap<String, ValueType> {
    let mut symbols = BTreeMap::from([("decode_srgb8".to_owned(), ValueType::DecodeTable)]);
    symbols.extend(literals.keys().map(|name| (name.clone(), ValueType::Real)));
    symbols.extend(
        enum_values
            .keys()
            .map(|name| (name.clone(), ValueType::Surround)),
    );
    symbols
}

fn insert_symbol(
    symbols: &mut BTreeMap<String, ValueType>,
    name: &str,
    value_type: ValueType,
) -> Result<(), String> {
    require(is_identifier(name), "invalid symbol name")?;
    require(
        symbols.insert(name.to_owned(), value_type).is_none(),
        "duplicate or shadowed symbol",
    )
}

fn fields(line: &str, expected: usize) -> Result<Vec<&str>, String> {
    let fields = line.split(' ').collect::<Vec<_>>();
    require(fields.len() == expected, "record arity mismatch")?;
    Ok(fields)
}

fn parse_finite_bits(token: &str) -> Result<u64, String> {
    require(
        token.len() == 16
            && token
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
        "binary64 payload is not 16 lowercase hexadecimal digits",
    )?;
    let bits = u64::from_str_radix(token, 16).map_err(|error| error.to_string())?;
    let value = f64::from_bits(bits);
    require(value.is_finite(), "binary64 payload is not finite")?;
    require(
        bits != (-0.0_f64).to_bits(),
        "negative zero is not canonical",
    )?;
    Ok(bits)
}

fn is_token_byte(byte: u8) -> bool {
    byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_')
}

fn is_identifier(value: &str) -> bool {
    value
        .bytes()
        .next()
        .is_some_and(|byte| byte.is_ascii_lowercase())
        && value.bytes().all(is_token_byte)
}

fn require(condition: bool, message: &str) -> Result<(), String> {
    if condition {
        Ok(())
    } else {
        Err(message.to_owned())
    }
}

fn validate_owner_literal_binding(formula: &Formula) -> Result<(), String> {
    const EXACT_REAL_LITERALS: [(&str, f64); 1] = [("zero", 0.0)];
    let owners: [(&str, &[(&str, f64)]); 5] = [
        ("exact-real SSA", &EXACT_REAL_LITERALS),
        ("sRGB/D65", srgb::contextual_region_formula_literals_v1()),
        ("CAT16", cat16::contextual_region_formula_literals_v1()),
        (
            "CAM16 viewing conditions",
            vc::contextual_region_formula_literals_v1(),
        ),
        (
            "CAM16/CAM16-UCS",
            cam16::contextual_region_formula_literals_v1(),
        ),
    ];
    let mut registered = BTreeMap::<&str, (u64, &str)>::new();
    for (owner, literals) in owners {
        for &(name, value) in literals {
            let bits = value.to_bits();
            if let Some(&(previous, previous_owner)) = registered.get(name) {
                if previous != bits {
                    return Err(format!(
                        "owner conflict for {name}: {previous_owner}={previous:016x}, {owner}={bits:016x}",
                    ));
                }
            } else {
                registered.insert(name, (bits, owner));
            }
        }
    }
    require(
        registered.len() == formula.literals.len(),
        "owner registry does not cover exactly the formula literal set",
    )?;
    for (name, artifact_bits) in &formula.literals {
        let Some(&(owner_bits, owner)) = registered.get(name.as_str()) else {
            return Err(format!("formula literal {name} has no numeric owner"));
        };
        if *artifact_bits != owner_bits {
            return Err(format!(
                "formula literal {name}={artifact_bits:016x} differs from {owner} owner={owner_bits:016x}",
            ));
        }
    }
    Ok(())
}

#[test]
fn exact_real_formula_has_one_strict_typed_canonical_parse() {
    let formula = parse_formula(FORMULA_BYTES).unwrap();
    validate_owner_literal_binding(&formula).unwrap();
    assert_eq!(emit_formula(&formula).as_bytes(), FORMULA_BYTES);
    assert_eq!(formula.decode.len(), 256);
    assert_eq!(formula.literals.len(), 56);
    assert_eq!(formula.enum_values.len(), 3);
    assert_eq!(formula.point.inputs.len(), 6);
    assert_eq!(formula.point.nodes.len(), 226);
    assert_eq!(formula.point.checkpoints.len(), 39);
    assert_eq!(formula.point.outputs.len(), 3);
    assert_eq!(formula.segment.inputs.len(), 14);
    assert_eq!(formula.segment.nodes.len(), 27);
    assert_eq!(formula.singleton.inputs.len(), 8);
    assert_eq!(formula.singleton.nodes.len(), 12);
    for (ordinal, bits) in formula.decode.into_iter().enumerate() {
        assert_eq!(bits, decode_8bit(ordinal as u8).to_bits());
    }
}

#[test]
fn exact_real_formula_parser_rejects_semantic_and_canonical_mutants() {
    for (needle, replacement) in [
        ("types 4", "types 5"),
        (
            "operator add 2 real exact_x_plus_y",
            "operator add 2 real exact_x_minus_y",
        ),
        (
            "node xyz_x_r real mul srgb_m00 linear_r",
            "node xyz_x_r real unknown srgb_m00 linear_r",
        ),
        (
            "node xyz_x_r real mul srgb_m00 linear_r",
            "node xyz_x_r real mul xyz_x_g linear_r",
        ),
        (
            "node xyz_x real add xyz_x_rg xyz_x_b",
            "node xyz_x real add xyz_x_r xyz_x_b",
        ),
        (
            "node n real add background_ratio zero",
            "node n real add p0_2 zero",
        ),
        (
            "literal zero 0000000000000000",
            "literal zero 7ff0000000000000",
        ),
        ("input surround surround_profile", "input surround u8"),
        (
            "enum surround_profile surround_dark 03",
            "enum surround_profile surround_dark 04",
        ),
        ("checkpoint n n", "checkpoint n background_ratio"),
        ("rule boundary inclusive", "rule boundary exclusive"),
    ] {
        let mutant = replace_once(FORMULA_BYTES, needle.as_bytes(), replacement.as_bytes());
        assert!(
            parse_formula(&mutant).is_err(),
            "mutant unexpectedly parsed: {replacement}",
        );
    }
    for mutant in [
        [FORMULA_BYTES, b"\n"].concat(),
        replace_once(FORMULA_BYTES, b"\n", b"\r\n"),
        replace_once(FORMULA_BYTES, b"types 4\n", b"types 4 \n"),
    ] {
        assert!(parse_formula(&mutant).is_err());
    }
}

#[test]
fn one_ulp_owner_literal_mutation_changes_identity_and_fails_exact_source_binding() {
    let mutant_bytes = replace_once(
        FORMULA_BYTES,
        b"literal p1_7 3ffb333333333333",
        b"literal p1_7 3ffb333333333334",
    );
    let mutant = parse_formula(&mutant_bytes).expect("well-typed semantic mutant must parse");
    assert_ne!(
        crate::sha256::digest(FORMULA_BYTES),
        crate::sha256::digest(&mutant_bytes),
    );

    let error = validate_owner_literal_binding(&mutant)
        .expect_err("one-ULP UCS J-scale drift must fail exact source binding");
    assert!(error.contains("p1_7"), "unexpected binding error: {error}");
}

#[test]
fn formula_decode_and_xyz_checkpoint_are_bit_bound_to_the_runtime_transform() {
    let formula = parse_formula(FORMULA_BYTES).unwrap();
    let channels = [0, 1, 10, 11, 127, 128, 254, 255];
    for red in channels {
        for green in channels {
            for blue in channels {
                let rgb = [red, green, blue];
                let values = evaluate_point_values(&formula, rgb, 64.0, 0.2, 1);
                let signal = ColorSignal::from_srgb8(Srgb8::new(rgb));
                let xyz = derive_modeled_tristimulus_v1(signal)
                    .unwrap()
                    .sample()
                    .xyz();
                assert_eq!(real(values["xyz_x"]).to_bits(), xyz[0].to_bits());
                assert_eq!(real(values["xyz_y"]).to_bits(), xyz[1].to_bits());
                assert_eq!(real(values["xyz_z"]).to_bits(), xyz[2].to_bits());
            }
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn segment_ssa_matches_an_independent_exact_integer_oracle(
        left in (
            -8_i64..=8,
            1_i64..=8,
            0_i64..=8,
            -8_i64..=8,
            -8_i64..=8,
            -8_i64..=8,
            -8_i64..=8,
        ),
        right in (
            -8_i64..=8,
            -8_i64..=8,
            0_i64..=8,
            0_i64..=8,
            1_i64..=4,
            -1_i64..=1,
            1_i64..=4,
        ),
    ) {
        let (t0, d, raw_beta, a, b, c0a, c0b) = left;
        let (c1a, c1b, rho0, rho1, g00, g01, g11) = right;
        prop_assume!(g00 * g11 > g01 * g01);
        let beta = raw_beta % (d + 1);
        let alpha = d - beta;
        let t = t0 + beta;
        let t1 = t0 + d;
        let formula = registered_formula();
        let values = evaluate_program(
            formula,
            &formula.segment,
            &[
                ("segment_t", Value::Real(t as f64)),
                ("segment_a", Value::Real(a as f64)),
                ("segment_b", Value::Real(b as f64)),
                ("segment_t0", Value::Real(t0 as f64)),
                ("segment_t1", Value::Real(t1 as f64)),
                ("segment_c0a", Value::Real(c0a as f64)),
                ("segment_c0b", Value::Real(c0b as f64)),
                ("segment_c1a", Value::Real(c1a as f64)),
                ("segment_c1b", Value::Real(c1b as f64)),
                ("segment_rho0", Value::Real(rho0 as f64)),
                ("segment_rho1", Value::Real(rho1 as f64)),
                ("segment_g00", Value::Real(g00 as f64)),
                ("segment_g01", Value::Real(g01 as f64)),
                ("segment_g11", Value::Real(g11 as f64)),
            ],
        );

        let d = i128::from(d);
        let alpha = i128::from(alpha);
        let beta = i128::from(beta);
        let ua = d * i128::from(a)
            - alpha * i128::from(c0a)
            - beta * i128::from(c1a);
        let ub = d * i128::from(b)
            - alpha * i128::from(c0b)
            - beta * i128::from(c1b);
        let quadratic = i128::from(g00) * ua * ua
            + 2 * i128::from(g01) * ua * ub
            + i128::from(g11) * ub * ub;
        let radius = d * (alpha * i128::from(rho0) + beta * i128::from(rho1));
        let oracle = (quadratic - radius) as f64;
        prop_assert_eq!(real(values["segment_f"]).to_bits(), oracle.to_bits());
    }

    #[test]
    fn singleton_ssa_matches_an_independent_exact_integer_oracle(
        values in (
            -8_i64..=8,
            -8_i64..=8,
            -8_i64..=8,
            -8_i64..=8,
            0_i64..=8,
            1_i64..=4,
            -1_i64..=1,
            1_i64..=4,
        ),
    ) {
        let (a, b, ca, cb, rho, g00, g01, g11) = values;
        prop_assume!(g00 * g11 > g01 * g01);
        let formula = registered_formula();
        let evaluated = evaluate_program(
            formula,
            &formula.singleton,
            &[
                ("singleton_a", Value::Real(a as f64)),
                ("singleton_b", Value::Real(b as f64)),
                ("singleton_ca", Value::Real(ca as f64)),
                ("singleton_cb", Value::Real(cb as f64)),
                ("singleton_rho", Value::Real(rho as f64)),
                ("singleton_g00", Value::Real(g00 as f64)),
                ("singleton_g01", Value::Real(g01 as f64)),
                ("singleton_g11", Value::Real(g11 as f64)),
            ],
        );
        let ua = i128::from(a - ca);
        let ub = i128::from(b - cb);
        let oracle = (i128::from(g00) * ua * ua
            + 2 * i128::from(g01) * ua * ub
            + i128::from(g11) * ub * ub
            - i128::from(rho)) as f64;
        prop_assert_eq!(real(evaluated["singleton_f"]).to_bits(), oracle.to_bits());
    }
}

fn replace_once(source: &[u8], needle: &[u8], replacement: &[u8]) -> Vec<u8> {
    let start = source
        .windows(needle.len())
        .position(|window| window == needle)
        .expect("registered mutation anchor is present");
    let mut result = Vec::with_capacity(source.len() - needle.len() + replacement.len());
    result.extend_from_slice(&source[..start]);
    result.extend_from_slice(replacement);
    result.extend_from_slice(&source[start + needle.len()..]);
    result
}

#[derive(Clone, Copy, Debug)]
enum Value {
    U8(u8),
    Real(f64),
    Bool(bool),
    Surround(u8),
    DecodeTable,
}

fn evaluate_point(
    formula: &Formula,
    rgb: [u8; 3],
    adapting_luminance: f64,
    background_ratio: f64,
    surround: u8,
) -> [f64; 3] {
    let values =
        evaluate_point_values(formula, rgb, adapting_luminance, background_ratio, surround);
    formula
        .point
        .outputs
        .iter()
        .map(|(name, _)| real(values[name]))
        .collect::<Vec<_>>()
        .try_into()
        .unwrap()
}

fn evaluate_point_values(
    formula: &Formula,
    rgb: [u8; 3],
    adapting_luminance: f64,
    background_ratio: f64,
    surround: u8,
) -> BTreeMap<String, Value> {
    evaluate_program(
        formula,
        &formula.point,
        &[
            ("r8", Value::U8(rgb[0])),
            ("g8", Value::U8(rgb[1])),
            ("b8", Value::U8(rgb[2])),
            ("adapting_luminance", Value::Real(adapting_luminance)),
            ("background_ratio", Value::Real(background_ratio)),
            ("surround", Value::Surround(surround)),
        ],
    )
}

fn evaluate_program(
    formula: &Formula,
    program: &Program,
    inputs: &[(&str, Value)],
) -> BTreeMap<String, Value> {
    let mut values = BTreeMap::new();
    values.insert("decode_srgb8".to_owned(), Value::DecodeTable);
    values.extend(
        formula
            .literals
            .iter()
            .map(|(name, bits)| (name.clone(), Value::Real(f64::from_bits(*bits)))),
    );
    values.extend(
        formula
            .enum_values
            .iter()
            .map(|(name, value)| (name.clone(), Value::Surround(*value))),
    );
    assert_eq!(inputs.len(), program.inputs.len());
    for ((name, value), (expected_name, expected_type)) in inputs.iter().zip(&program.inputs) {
        assert_eq!(*name, expected_name);
        assert_eq!(value_type(*value), *expected_type);
        assert!(values.insert((*name).to_owned(), *value).is_none());
    }
    for node in &program.nodes {
        let arguments = node
            .arguments
            .iter()
            .map(|name| values[name])
            .collect::<Vec<_>>();
        let value = evaluate_operation(&node.operator, &arguments, &formula.decode);
        assert_eq!(value_type(value), node.result);
        assert!(values.insert(node.name.clone(), value).is_none());
    }
    values
}

fn evaluate_operation(operator: &str, values: &[Value], decode: &[u64; 256]) -> Value {
    let result = match (operator, values) {
        ("lookup", [Value::DecodeTable, Value::U8(value)]) => {
            Value::Real(f64::from_bits(decode[usize::from(*value)]))
        }
        ("eq", [Value::U8(left), Value::U8(right)]) => Value::Bool(left == right),
        ("eq", [Value::Real(left), Value::Real(right)]) => Value::Bool(left == right),
        ("eq", [Value::Bool(left), Value::Bool(right)]) => Value::Bool(left == right),
        ("eq", [Value::Surround(left), Value::Surround(right)]) => Value::Bool(left == right),
        ("select", [Value::Bool(condition), left, right]) => {
            if *condition {
                *left
            } else {
                *right
            }
        }
        ("add", [left, right]) => Value::Real(real(*left) + real(*right)),
        ("sub", [left, right]) => Value::Real(real(*left) - real(*right)),
        ("mul", [left, right]) => Value::Real(real(*left) * real(*right)),
        ("div", [left, right]) => {
            let denominator = real(*right);
            assert_ne!(denominator, 0.0, "formula div domain is unproven");
            Value::Real(real(*left) / denominator)
        }
        ("min", [left, right]) => Value::Real(real(*left).min(real(*right))),
        ("max", [left, right]) => Value::Real(real(*left).max(real(*right))),
        ("root3", [value]) => {
            let value = real(*value);
            assert!(value >= 0.0, "formula root3 domain is unproven");
            Value::Real(value.cbrt())
        }
        ("sqrt", [value]) => {
            let value = real(*value);
            assert!(value >= 0.0, "formula sqrt domain is unproven");
            Value::Real(value.sqrt())
        }
        ("exp", [value]) => Value::Real(real(*value).exp()),
        ("log", [value]) => {
            let value = real(*value);
            assert!(value > 0.0, "formula log domain is unproven");
            Value::Real(value.ln())
        }
        ("sin", [value]) => Value::Real(real(*value).sin()),
        ("cos", [value]) => Value::Real(real(*value).cos()),
        ("abs", [value]) => Value::Real(real(*value).abs()),
        ("sign", [value]) => Value::Real(if real(*value) < 0.0 {
            -1.0
        } else if real(*value) > 0.0 {
            1.0
        } else {
            0.0
        }),
        ("pow_pos", [left, right]) => {
            let (left, right) = (real(*left), real(*right));
            assert!(left > 0.0, "formula pow_pos domain is unproven");
            Value::Real((right * left.ln()).exp())
        }
        ("pow_nn", [left, right]) => {
            let (left, right) = (real(*left), real(*right));
            if left == 0.0 && right > 0.0 {
                Value::Real(0.0)
            } else {
                assert!(left > 0.0, "formula pow_nn domain is unproven");
                Value::Real((right * left.ln()).exp())
            }
        }
        ("ratio0", [left, right]) => {
            let (left, right) = (real(*left), real(*right));
            Value::Real(if left == 0.0 && right == 0.0 {
                0.0
            } else {
                assert!(right > 0.0, "formula ratio0 domain is unproven");
                left / right
            })
        }
        _ => panic!("typed parser admitted an unevaluable operation {operator}"),
    };
    match result {
        Value::Real(value) => {
            assert!(
                value.is_finite(),
                "binary64 characterization left the finite domain"
            );
            Value::Real(if value == 0.0 { 0.0 } else { value })
        }
        other => other,
    }
}

fn value_type(value: Value) -> ValueType {
    match value {
        Value::U8(_) => ValueType::U8,
        Value::Real(_) => ValueType::Real,
        Value::Bool(_) => ValueType::Bool,
        Value::Surround(_) => ValueType::Surround,
        Value::DecodeTable => ValueType::DecodeTable,
    }
}

fn real(value: Value) -> f64 {
    let Value::Real(value) = value else {
        panic!("typed formula value is not real");
    };
    value
}

fn production_point(
    rgb: [u8; 3],
    adapting_luminance: f64,
    background_ratio: f64,
    surround: SurroundProfileId,
) -> [f64; 3] {
    let context = AppearanceContextId::from_inputs(
        AppearanceContextSchemaReleaseId::Ciecam16ViewingInputsV1,
        IEC_SRGB_D65_XYZ_FRAME_V1,
        AdaptingLuminanceCdM2::try_new(adapting_luminance).unwrap(),
        BackgroundLuminanceRatio::try_new(background_ratio).unwrap(),
        surround,
    );
    let signal = ColorSignal::from_srgb8(Srgb8::new(rgb));
    let sample = derive_modeled_tristimulus_v1(signal).unwrap().sample();
    let occurrence = LcsOccurrence::in_context(sample, context).unwrap();
    let view = AppearanceState::derive_v1(occurrence)
        .unwrap()
        .cam16_ucs()
        .unwrap();
    [view.j_prime(), view.a_prime(), view.b_prime()]
}

fn ordered_bits(value: f64) -> u64 {
    let bits = value.to_bits();
    if bits >> 63 == 0 {
        bits | (1_u64 << 63)
    } else {
        !bits
    }
}

fn ulp_distance(left: f64, right: f64) -> u64 {
    ordered_bits(left).abs_diff(ordered_bits(right))
}

#[test]
fn formula_point_graph_matches_the_registered_binary64_characterization() {
    let formula = parse_formula(FORMULA_BYTES).unwrap();
    let channels = [0, 1, 10, 11, 127, 128, 254, 255];
    let contexts = [(1.0, 0.01), (64.0, 0.2), (1000.0, 0.9)];
    let surrounds = [
        (SurroundProfileId::AverageV1, 1),
        (SurroundProfileId::DimV1, 2),
        (SurroundProfileId::DarkV1, 3),
    ];
    let mut maximum = [0_u64; 3];
    let mut maximum_absolute = [0.0_f64; 3];
    let mut maximum_relative = [0.0_f64; 3];
    let mut witness = [([0_u8; 3], 0.0, 0.0, SurroundProfileId::AverageV1); 3];
    let mut witness_values = [([0.0_f64; 3], [0.0_f64; 3]); 3];
    for red in channels {
        for green in channels {
            for blue in channels {
                for (adapting_luminance, background_ratio) in contexts {
                    for (surround, tag) in surrounds {
                        let rgb = [red, green, blue];
                        let lifted = evaluate_point(
                            &formula,
                            rgb,
                            adapting_luminance,
                            background_ratio,
                            tag,
                        );
                        let production =
                            production_point(rgb, adapting_luminance, background_ratio, surround);
                        for coordinate in 0..3 {
                            let distance = ulp_distance(lifted[coordinate], production[coordinate]);
                            let absolute = (lifted[coordinate] - production[coordinate]).abs();
                            let relative = absolute
                                / lifted[coordinate]
                                    .abs()
                                    .max(production[coordinate].abs())
                                    .max(f64::MIN_POSITIVE);
                            maximum_absolute[coordinate] =
                                maximum_absolute[coordinate].max(absolute);
                            maximum_relative[coordinate] =
                                maximum_relative[coordinate].max(relative);
                            if distance > maximum[coordinate] {
                                maximum[coordinate] = distance;
                                witness[coordinate] =
                                    (rgb, adapting_luminance, background_ratio, surround);
                                witness_values[coordinate] = (lifted, production);
                            }
                        }
                    }
                }
            }
        }
    }
    eprintln!(
        "formula/runtime ULP maxima: {maximum:?}; abs: {maximum_absolute:?}; rel: {maximum_relative:?}; witnesses: {witness:?}; values: {witness_values:?}",
    );
    // Это empirical binary64 ratchet, не физический допуск и не доказательство
    // exact-real эквивалентности. Первичный registered reduced corpus 8³ × 3 × 3 на macOS
    // дал [4.264e-14, 4.746e-13, 9.975e-14]; десятичные ceilings округлены вверх
    // и затем проверяются тем же тестом на CI Linux. Изменение требует нового
    // corpus/artifact receipt. V5b2c Arb ∥ MPFI доказывает all-domain
    // classification только относительно этого artifact; связь artifact ↔ numeric owners
    // остаётся отдельным exact source-binding обязательством выше.
    let absolute_ratcheted_ceilings = [1.0e-13, 1.0e-12, 2.0e-13];
    for coordinate in 0..3 {
        assert!(
            maximum_absolute[coordinate] <= absolute_ratcheted_ceilings[coordinate],
            "coordinate {coordinate} binary64 characterization drifted at {:?}: {} > {}",
            witness[coordinate],
            maximum_absolute[coordinate],
            absolute_ratcheted_ceilings[coordinate],
        );
    }
}
