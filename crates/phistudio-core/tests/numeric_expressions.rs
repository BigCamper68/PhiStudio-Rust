use phistudio_core::Formula;

#[test]
fn constant_expressions_share_the_bounded_parser_without_accepting_time_or_code() {
    for (source, expected) in [
        ("675/2", 337.5),
        ("360/7", 360. / 7.),
        ("sin(pi/4)*300", 300. / 2_f64.sqrt()),
        ("2^3^2", 512.),
        ("-2^2", -4.),
        ("1e-3 + .25", 0.251),
        ("clamp(8, 1, 5)", 5.),
        ("lerp(10,30,.25)", 15.),
    ] {
        assert!(
            (Formula::constant(source).unwrap() - expected).abs() < 1e-10,
            "{source}"
        );
    }
    for source in [
        "",
        "1/0",
        "sqrt(-1)",
        "1e309",
        "2+",
        "t",
        "0*t",
        "sin(t)",
        "min(1,1/0)+t",
        "alert(1)",
        "foo.bar",
        "[1]",
        "2;3",
        "pow(2)",
    ] {
        assert!(Formula::constant(source).is_err(), "{source}");
    }
    assert!(Formula::constant(&"1+".repeat(3000)).is_err());
    assert!(Formula::constant(&format!("{}1{}", "(".repeat(100), ")".repeat(100))).is_err());
    assert_eq!(Formula::parse("t*2").unwrap().sample(3.).unwrap(), 6.);
}
