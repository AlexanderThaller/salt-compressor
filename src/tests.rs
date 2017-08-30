mod test_retcode {
    use Retcode;

    #[test]
    fn from_success() {
        assert_eq!(Retcode::Success, 0.into())
    }

    #[test]
    fn from_failure() {
        for i in 1..10 {
            assert_eq!(Retcode::Failure, i.into())
        }
    }
}

mod test_get_results {
    extern crate serde_json;
    use cleanup_input_data;
    use get_results;
    use MinionResult;
    use Retcode;
    use serde_json::Value;
    use std::collections::BTreeMap as DataMap;

    #[test]
    #[should_panic(expected = "value it not an object")]
    fn value_not_an_object() {
        let value = Value::default();

        match get_results(&value, DataMap::default()) {
            Ok(_) => {}
            Err(e) => panic!(format!("{}", e)),
        }
    }

    #[test]
    fn empty_results() {
        let value: Value = serde_json::from_str("{}").unwrap();

        let got = match get_results(&value, DataMap::default()) {
            Ok(r) => r,
            Err(e) => panic!("unexpected error: {}", e),
        };
        let expected = Vec::new();

        trace!("got: {:#?}", got);
        trace!("expected: {:#?}", expected);

        assert_eq!(got, expected);
    }

    #[test]
    fn only_failed_hosts() {
        let input = include_str!("../testdata/only_failed_hosts.json");
        let (input, _) = cleanup_input_data(input.to_owned());

        let value: Value =
            serde_json::from_str(input.as_str()).expect("can not parse input to json");

        let mut failed_hosts = DataMap::default();
        failed_hosts.insert("minion_fail_1".into(), "");
        failed_hosts.insert("minion_fail_1".into(), "");

        let got = match get_results(&value, failed_hosts.clone()) {
            Ok(r) => r,
            Err(e) => panic!("unexpected error: {}", e),
        };
        let mut expected = Vec::new();
        for (host, message) in failed_hosts {
            expected.push(MinionResult {
                host: host,
                retcode: Retcode::Failure,
                output: Some(message.into()),
                ..MinionResult::default()
            });
        }

        println!("got: {:#?}", got);
        println!("expected: {:#?}", expected);

        assert_eq!(got, expected);
    }

    #[test]
    fn duplicate_keys_hosts() {
        let input = include_str!("../testdata/duplicate_keys_hosts.json");
        let (input, _) = cleanup_input_data(input.to_owned());

        let value: Value =
            serde_json::from_str(input.as_str()).expect("can not parse input to json");

        let mut failed_hosts = DataMap::default();
        failed_hosts.insert("minion_fail_1".into(), "");
        failed_hosts.insert("minion_fail_2".into(), "");

        let got = match get_results(&value, failed_hosts.clone()) {
            Ok(r) => r,
            Err(e) => panic!("unexpected error: {}", e),
        };

        let mut expected = Vec::new();
        for (host, message) in failed_hosts {
            expected.push(MinionResult {
                host: host,
                retcode: Retcode::Failure,
                output: Some(message.to_string()),
                ..MinionResult::default()
            });
        }

        println!("got: {:#?}", got);
        println!("expected: {:#?}", expected);

        assert_eq!(got, expected);
    }


    #[test]
    fn array() {
        let input = include_str!("../testdata/array.json");
        let value: Value = serde_json::from_str(input).unwrap();

        let got = match get_results(&value, DataMap::default()) {
            Ok(r) => r,
            Err(e) => panic!("unexpected error: {}", e),
        };

        let mut expected = Vec::new();
        expected.push(MinionResult {
            host: "minion".to_string(),
            retcode: Retcode::Failure,
            result: Some("line1\nline2\nline3".to_string()),
            ..MinionResult::default()
        });

        trace!("got: {:#?}", got);
        trace!("expected: {:#?}", expected);

        assert_eq!(got, expected);
    }

    #[test]
    #[should_panic(expected = "can not convert the array value to a string")]
    fn array_weird() {
        let input = include_str!("../testdata/array_weird.json");
        let value: Value = serde_json::from_str(input).unwrap();

        match get_results(&value, DataMap::default()) {
            Ok(_) => {}
            Err(e) => panic!(e),
        };
    }

    #[test]
    fn bool() {
        let input = include_str!("../testdata/bool.json");
        let value: Value = serde_json::from_str(input).unwrap();

        let got = match get_results(&value, DataMap::default()) {
            Ok(r) => r,
            Err(e) => panic!("unexpected error: {}", e),
        };

        let mut expected = Vec::new();
        expected.push(MinionResult {
            host: "minion".to_string(),
            retcode: Retcode::Success,
            result: Some("true".to_string()),
            ..MinionResult::default()
        });
        expected.push(MinionResult {
            host: "minion_fail".to_string(),
            retcode: Retcode::Failure,
            result: Some("false".to_string()),
            ..MinionResult::default()
        });
        expected.sort();

        trace!("got: {:#?}", got);
        trace!("expected: {:#?}", expected);

        assert_eq!(got, expected);
    }

    #[test]
    fn not_ret_array() {
        let input = include_str!("../testdata/no_ret_array.json");
        let value: Value = serde_json::from_str(input).unwrap();

        let got = match get_results(&value, DataMap::default()) {
            Ok(r) => r,
            Err(e) => panic!("unexpected error: {}", e),
        };

        let mut expected = Vec::new();
        expected.push(MinionResult {
            host: "minion".to_string(),
            retcode: Retcode::Failure,
            result: Some("line1\nline2\nline3".to_string()),
            ..MinionResult::default()
        });

        trace!("got: {:#?}", got);
        trace!("expected: {:#?}", expected);

        assert_eq!(got, expected);
    }
}
