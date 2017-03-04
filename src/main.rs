#[macro_use]
extern crate log;
extern crate loggerv;

#[macro_use]
extern crate clap;

extern crate serde_json;

extern crate time;

extern crate regex;

extern crate colored;

use clap::App;
use colored::*;
use log::LogLevel;
use regex::Regex;
use serde_json::Value;
use std::collections::BTreeMap as DataMap;
use std::fmt;
use std::fs::File;
use std::io::{self, Read};
use std::io::Write;
use std::process;
use time::get_time;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Default)]
struct MinionResult {
    command: Option<String>,
    retcode: Retcode,
    output: Option<String>,
    result: Option<String>,
    host: String,
}

type MinionResults = Vec<MinionResult>;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Retcode {
    Success,
    Failure,
}

impl Retcode {
    fn is_success(&self) -> bool {
        self == &Retcode::Success
    }
}

impl Default for Retcode {
    fn default() -> Retcode {
        Retcode::Failure
    }
}

impl From<u64> for Retcode {
    fn from(input: u64) -> Self {
        if input == 0 {
            Retcode::Success
        } else {
            Retcode::Failure
        }
    }
}

fn main() {
    let yaml = load_yaml!("cli.yml");
    let matches = App::from_yaml(yaml)
        .version(crate_version!())
        .get_matches();
    trace!("matches: {:?}", matches);

    {
        let loglevel: LogLevel = value_t!(matches, "loglevel", LogLevel)
            .expect("can not parse loglevel from args");
        loggerv::init_with_level(loglevel).expect("can not initialize logger with parsed loglevel");
    }

    let changed = matches.is_present("changed");
    let no_save_file = matches.is_present("no_save_file");

    let (host_data, no_return) = {
        let input = matches.value_of("input").expect("can not get input file from args");

        let input = if input == "-" {
            let mut buffer = String::new();
            io::stdin().read_to_string(&mut buffer).expect("can not read from stdin");
            buffer
        } else {
            let mut file = File::open(input).expect("can not open input file");
            let mut input = String::new();
            file.read_to_string(&mut input).expect("can not read input file to string");
            input
        };

        // match all hosts that have not returned as they are not in the json data
        // format is normally like "Minion pricesearch did not respond. No job will be sent."
        let catch_not_returned_minions =
            Regex::new(r"(?m)^Minion (\S*) did not respond\. No job will be sent\.$")
                .expect("regex for catching not returned minions is not valid");

        let mut no_return = Vec::new();
        for host in catch_not_returned_minions.captures_iter(input.as_str()) {
            no_return.push(host[1].to_string());
        }

        // clean up hosts that have not returned from the json data
        (catch_not_returned_minions.replace_all(input.as_str(), "").into_owned(), no_return)
    };

    let value: Value = serde_json::from_str(host_data.as_str())
        .expect("can not convert input data to value. have you run the salt command with \
                 --static?");

    let results = match get_results(&value, no_return) {
        Ok(r) => r,
        Err(e) => {
            error!("can not get results from serde value: {}", e);
            if !no_save_file {
                write_save_file(host_data.as_str());
            }
            process::exit(1)
        }
    };

    trace!("results: {:#?}", results);

    let compressed = get_compressed(results);
    trace!("compressed: {:#?}", compressed);

    print_compressed(compressed, changed);
}

#[derive(Debug)]
enum ResultError {
    ConvertDiffToString,
    ConvertValueToString,
    ReturnCodeNotNumber,
    RetValueIsNone,
    RetValueIsNull,
    RetValueIsNumber,
    ValueNotAnObject,
}

impl fmt::Display for ResultError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            ResultError::ConvertDiffToString => write!(f, "can not convert diff to string"),
            ResultError::ConvertValueToString => write!(f, "can not convert value to string"),
            ResultError::ReturnCodeNotNumber => write!(f, "returncode is not a number"),
            ResultError::RetValueIsNone => write!(f, "ret value is none"),
            ResultError::RetValueIsNull => write!(f, "ret value is null"),
            ResultError::RetValueIsNumber => write!(f, "ret value is number"),
            ResultError::ValueNotAnObject => write!(f, "value it not an object"),
        }
    }
}

fn get_results(value: &Value, no_return: Vec<String>) -> Result<MinionResults, ResultError> {
    if !value.is_object() {
        return Err(ResultError::ValueNotAnObject);
    }

    let mut results: MinionResults = Vec::new();

    for (host, values) in value.as_object().unwrap().iter() {
        trace!("host: {:#?}", host);
        trace!("values: {:#?}", values);

        let retcode: Retcode = match values.get("retcode") {
            Some(o) => {
                match o.as_u64() {
                    Some(v) => v.into(),
                    None => return Err(ResultError::ReturnCodeNotNumber),
                }
            }
            None => {
                warn!("host {} does not have a return code", host);
                Retcode::Failure
            }
        };

        let ret = match values.get("ret") {
            None => return Err(ResultError::RetValueIsNone),
            Some(r) => r,
        };

        match *ret {
            Value::Null => return Err(ResultError::RetValueIsNull),
            Value::Bool(r) => {
                let (command, result, output) = if r {
                    (None, Some("true".to_string()), None)
                } else {
                    (None, Some("false".to_string()), None)
                };

                results.push(MinionResult {
                    command: command,
                    host: host.clone(),
                    output: output,
                    result: result,
                    retcode: retcode,
                });
            }
            Value::Number(_) => return Err(ResultError::RetValueIsNumber),
            Value::String(ref r) => {
                results.push(MinionResult {
                    host: host.clone(),
                    result: Some(r.clone()),
                    retcode: retcode,
                    ..MinionResult::default()
                });
            }

            Value::Array(ref r) => {
                let values: Vec<_> = r.iter()
                    .map(|v| v.as_str().expect("can not convert the array value to a string"))
                    .collect();

                results.push(MinionResult {
                    host: host.clone(),
                    result: Some(values.join("\n").to_string()),
                    retcode: retcode.clone(),
                    ..MinionResult::default()
                });
            }
            Value::Object(ref r) => {
                if r.is_empty() {
                    results.push(MinionResult {
                        host: host.clone(),
                        retcode: retcode.clone(),
                        ..MinionResult::default()
                    });
                }

                for (command, command_result) in r.iter() {
                    trace!("command: {:#?}", command);
                    trace!("command_result: {:#?}", command_result);

                    let result = match command_result.get("comment") {
                        Some(r) => {
                            match r.as_str() {
                                Some(s) => Some(s.to_string()),
                                None => return Err(ResultError::ConvertValueToString),
                            }
                        }
                        None => None,
                    };

                    let output = match command_result.get("changes") {
                        Some(r) => {
                            match r.get("diff") {
                                Some(d) => {
                                    match d.as_str() {
                                        Some(i) => Some(i.to_string()),
                                        None => return Err(ResultError::ConvertDiffToString),
                                    }
                                }
                                None => None,
                            }
                        }
                        None => None,
                    };

                    results.push(MinionResult {
                        command: Some(command.to_string()),
                        host: host.clone(),
                        output: output,
                        result: result,
                        retcode: retcode.clone(),
                    });
                }
            }
        };
    }

    for host in no_return {
        results.push(MinionResult {
            host: host,
            retcode: Retcode::Failure,
            output: Some("Minion did not respond. No job will be sent.".to_string()),
            ..MinionResult::default()
        });
    }

    Ok(results)
}

fn get_compressed(results: MinionResults) -> DataMap<MinionResult, Vec<String>> {
    // compress output by changeing the hostname to the same value for all results and then just
    // adding all hosts with that value to the map.
    let mut compressed: DataMap<MinionResult, Vec<String>> = DataMap::new();
    for result in results {
        let mut result_no_host = result.clone();
        result_no_host.host = String::new();

        compressed.entry(result_no_host).or_insert_with(Vec::new).push(result.host);
    }

    compressed
}

fn print_compressed(compressed: DataMap<MinionResult, Vec<String>>, changed: bool) {
    let mut unchanged = 0;
    for (result, hosts) in compressed {
        // continue if we only want to print out changes and there are none and the command was a
        // success
        // TODO: make this a filter of the map
        if changed && !result.output.is_some() && result.retcode.is_success() {
            unchanged += 1;
            continue;
        }

        println!("");
        println!("{}", "----------".bold());
        println!("");

        // state, command info
        {
            if result.command.is_some() {
                println!("{}", "------".purple());

                if result.command.is_some() {
                    println!("{}",
                             format!("COMMAND: {}", result.command.clone().unwrap()).purple());
                }

                println!("{}\n", "------".purple());
            }
        }

        // hosts
        {
            println!("{}", "------".cyan());
            println!("{}{}", "HOSTS: ".cyan(), hosts.join(", ").as_str());
            println!("{}\n", "------".cyan());
        }


        // output
        {
            println!("{}", "------".yellow());

            match result.retcode {
                Retcode::Success => println!("{}{}", "RETURN CODE: ".yellow(), "Success".green()),
                Retcode::Failure => println!("{}{}", "RETURN CODE: ".yellow(), "Failure".red()),
            }

            if result.result.is_some() {
                println!("{}", "RESULT:".yellow());
                println!("{}\n", result.result.unwrap());
            }

            println!("{}", "OUTPUT:".yellow());
            if result.output.is_some() {
                for line in result.output.unwrap().lines() {
                    if line.starts_with('-') {
                        println!("{}", line.red());
                        continue;
                    }

                    if line.starts_with('+') {
                        println!("{}", line.green());
                        continue;
                    }

                    println!("{}", line);
                }
            } else {
                println!("No changes");
            }
            println!("{}", "------".yellow());
        }
    }

    if changed {
        println!("");
        info!("{} unchanged states", unchanged);
    }
}

fn write_save_file(host_data: &str) {
    let save_filename = format!("/tmp/salt-compressor_{}.json", get_time().sec);
    let mut save_file = File::create(save_filename.clone()).expect("can not create save_file");
    save_file.write_all(host_data.as_bytes()).expect("can not write host data to save_file");
    info!("please send me the save file under {} which contains the json \
           data from salt",
          save_filename);
}
