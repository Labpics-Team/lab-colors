use super::*;
use serde_json::{Value,json};
use std::io::Cursor;

const GOOD:&str=include_str!("../examples/declared-point.json");
const REJECTED:&str=include_str!("../examples/rejected-point.json");
fn invoke(args:&[&str],input:&[u8])->(i32,Vec<u8>,Vec<u8>) {
    let (mut out,mut err)=(Vec::new(),Vec::new());
    let code=run(args.iter().map(OsString::from),Cursor::new(input),&mut out,&mut err);
    (code,out,err)
}
fn failure(input:&[u8],exit:i32,code:&str) {
    let (status,out,err)=invoke(&[],input);
    assert_eq!(status,exit,"{}",String::from_utf8_lossy(&err));
    assert!(out.is_empty());
    let document:Value=serde_json::from_slice(&err).unwrap();
    assert_eq!(document["formatVersion"],1);
    assert_eq!(document["ok"],false);
    assert_eq!(document["error"]["code"],code);
    assert_eq!(document.as_object().unwrap().len(),3);
}
fn changed(edit:impl FnOnce(&mut Value))->Vec<u8> {
    let mut value:Value=serde_json::from_str(GOOD).unwrap();edit(&mut value);serde_json::to_vec(&value).unwrap()
}

#[test]
fn documented_request_returns_only_declared_modeled_report_and_lcen() {
    let (code,out,err)=invoke(&[],GOOD.as_bytes());
    assert_eq!(code,0,"{}",String::from_utf8_lossy(&err));assert!(err.is_empty());
    let report:Value=serde_json::from_slice(&out).unwrap();
    assert_eq!(report["kind"],REPORT_KIND);
    assert_eq!(report["terminalSrgb8"],json!([128,128,128]));
    assert_eq!(report["scope"],SCOPE);assert_eq!(report["admission"],ADMISSION);
    assert_eq!(report["human"],HUMAN);assert_eq!(report["rendererProvenance"],"unverified");
    assert!(report["certificateHex"].as_str().unwrap().starts_with("4c43454e"));
    assert!(out.len()<=MAX_OUTPUT_BYTES);
}
#[test]
fn final_composite_rejection_never_leaks_partial_success() {
    failure(REJECTED.as_bytes(),4,"rejected_by_convention");
    failure(&changed(|v|v["observation"]["scenarios"][0]["surfaces"][0]=json!([0,0,0])),4,"materialization_not_ready");
}
#[test]
fn no_profile_unknown_fields_duplicates_and_wrong_types_fail_closed() {
    let cases=[
        changed(|v|{v.as_object_mut().unwrap().remove("profile");}),
        changed(|v|v["extra"]=json!(true)),
        changed(|v|v["binding"]["extra"]=json!(0)),
        changed(|v|v["profile"]["human"]=json!(true)),
        changed(|v|v["observation"]["revision"]=json!(-1)),
        changed(|v|v["observation"]["revision"]=json!(0.5)),
        changed(|v|v["observation"]["scenarios"][0]["surfaces"][0]=json!([1,2,256])),
        changed(|v|v["observation"]["scenarios"][0]["surfaces"][0]=json!([1,2])),
        GOOD.replacen("\"formatVersion\": 1","\"formatVersion\": 1, \"formatVersion\": 1",1).into_bytes(),
        GOOD.replacen("\"scope\":", "\"scope\":\"whole-field\",\"scope\":",1).into_bytes(),
        [GOOD,GOOD].concat().into_bytes(),
        b"null".to_vec(),Vec::new(),vec![0xff],
    ];
    for input in cases {failure(&input,2,"invalid_document");}
}
#[test]
fn unsupported_profile_and_release_do_not_select_defaults() {
    for (input,code) in [
        (changed(|v|v["formatVersion"]=json!(2)),"unsupported_request"),
        (changed(|v|v["kind"]=json!("other")),"unsupported_request"),
        (changed(|v|v["profile"]["scope"]=json!("whole-field")),"unsupported_scope"),
        (changed(|v|v["profile"]["admission"]=json!("production-auto")),"unsupported_admission"),
        (changed(|v|v["profile"]["human"]=json!("confirmed")),"unsupported_human_evidence"),
        (changed(|v|v["profile"]["conventionReleaseSha256"]=json!("0".repeat(64))),"unsupported_convention_release"),
    ] {failure(&input,3,code);}
}
#[test]
fn canonical_wire_and_observation_shape_are_not_silently_repaired() {
    failure(&changed(|v|v["programWireHex"]=json!("0")),2,"invalid_hex");
    failure(&changed(|v|v["programWireHex"]=json!("AB")),2,"invalid_hex");
    failure(&changed(|v|v["programWireHex"]=json!("00")),2,"invalid_program_wire");
    failure(&changed(|v|v["observation"]["scenarios"]=json!([])),2,"missing_scenarios");
    failure(&changed(|v|v["binding"]["outputSlot"]=json!(1000)),3,"attachment_binding_rejected");
    failure(&changed(|v|v["observation"]["scenarios"][0]["surfaces"]=json!([])),4,"observation_rejected");
}
#[test]
fn collection_limits_stop_at_the_declared_boundary() {
    let scenarios=changed(|v|{
        v["observation"]["scenarios"]=Value::Array((0..65).map(|i|json!({"id":i,"surfaces":[[128,128,127]]})).collect());
    });
    failure(&scenarios,2,"invalid_document");
    let surfaces=changed(|v|v["observation"]["scenarios"][0]["surfaces"]=json!(vec![[128,128,127];4097]));
    failure(&surfaces,2,"invalid_document");
    // На точной границе парсер принимает данные; семантика остаётся у Program.
    let at_limit=changed(|v|v["observation"]["scenarios"][0]["surfaces"]=json!(vec![[128,128,127];4096]));
    assert!(serde_json::from_slice::<Request>(&at_limit).is_ok());
}
#[test]
fn reader_consumes_at_most_the_limit_plus_one_and_accepts_exact_boundary() {
    struct Infinite{read:usize}
    impl Read for Infinite {
        fn read(&mut self,out:&mut [u8])->io::Result<usize>{out.fill(b' ');self.read+=out.len();Ok(out.len())}
    }
    let mut source=Infinite{read:0};
    assert_eq!(read_bounded(&mut source).unwrap_err(),Error::resource("input_too_large"));
    assert_eq!(source.read,MAX_INPUT_BYTES+1);
    let mut exact=vec![b' ';MAX_INPUT_BYTES-GOOD.len()];exact.extend_from_slice(GOOD.as_bytes());
    let (code,_,err)=invoke(&[],&exact);assert_eq!(code,0,"{}",String::from_utf8_lossy(&err));
    exact.push(b' ');failure(&exact,6,"input_too_large");
}
#[test]
fn jsonl_is_one_record_and_format_flags_are_not_ambiguous() {
    let input=serde_json::to_vec(&serde_json::from_str::<Value>(GOOD).unwrap()).unwrap();
    let (code,out,err)=invoke(&["--format","jsonl"],&input);
    assert_eq!(code,0,"{}",String::from_utf8_lossy(&err));assert_eq!(out.iter().filter(|&&b|b==b'\n').count(),1);
    assert!(serde_json::from_slice::<Value>(&out).unwrap()["ok"].as_bool().unwrap());
    let (code,out,err)=invoke(&["--format","jsonl"],GOOD.as_bytes());
    assert_eq!(code,2);assert!(out.is_empty());assert!(String::from_utf8_lossy(&err).contains("invalid_jsonl_record"));
    for args in [vec!["--format","json","--format","json"],vec!["--format"],vec!["--help","-"],vec!["--force"],vec!["a","b"]] {
        let (code,out,_)=invoke(&args,b"");assert_eq!(code,2);assert!(out.is_empty());
    }
}
#[test]
fn stream_failure_cannot_report_success_or_publish_a_certificate() {
    struct BadRead;
    impl Read for BadRead{fn read(&mut self,_:&mut[u8])->io::Result<usize>{Err(io::Error::other("private path"))}}
    let (mut out,mut err)=(Vec::new(),Vec::new());
    assert_eq!(run(Vec::<OsString>::new(),BadRead,&mut out,&mut err),5);
    assert!(out.is_empty());assert!(!String::from_utf8_lossy(&err).contains("private path"));
    struct Partial{written:Vec<u8>}
    impl Write for Partial {
        fn write(&mut self,b:&[u8])->io::Result<usize>{if self.written.is_empty(){self.written.extend_from_slice(&b[..7]);Ok(7)}else{Err(io::Error::other("closed"))}}
        fn flush(&mut self)->io::Result<()>{Ok(())}
    }
    let mut partial=Partial{written:vec![]};err.clear();
    assert_eq!(run(Vec::<OsString>::new(),GOOD.as_bytes(),&mut partial,&mut err),5);
    assert_eq!(partial.written.len(),7);assert!(serde_json::from_slice::<Value>(&partial.written).is_err());
    assert!(String::from_utf8_lossy(&err).contains("write_failed"));
}
#[test]
fn semantic_results_are_complete_before_writing_and_help_never_reads_input() {
    let mut buffer=OutputBuffer::default();assert!(buffer.write_all(&[0;MAX_OUTPUT_BYTES]).is_ok());
    assert!(buffer.write_all(&[0]).is_err());assert_eq!(buffer.0.len(),MAX_OUTPUT_BYTES);
    struct NoRead;
    impl Read for NoRead{fn read(&mut self,_:&mut[u8])->io::Result<usize>{panic!("help must not read stdin")}}
    let (mut out,mut err)=(Vec::new(),Vec::new());
    assert_eq!(run([OsString::from("--help")],NoRead,&mut out,&mut err),0);
    assert!(String::from_utf8_lossy(&out).contains("labcolors-evaluate"));assert!(err.is_empty());
}
