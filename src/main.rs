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
use std::fs::File;
use std::io::{self, Read};
use std::io::Write;
use time::get_time;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Default)]
struct Result {
    command: Option<String>,
    retcode: Retcode,
    output: Option<String>,
    result: Option<String>,
    host: String,
}

type Results = Vec<Result>;

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

    let host_data = {
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

        for host in catch_not_returned_minions.captures_iter(input.as_str()) {
            warn!("host {} did not return", &host[1]);
        }

        // clean up hosts that have not returned from the json data
        catch_not_returned_minions.replace_all(input.as_str(), "").into_owned()
    };

    // TODO: Make functions use errors instead of panicing and only write a save file if there is
    // an error
    let save_filename = format!("/tmp/salt-compressor_{}.json", get_time().sec);
    let mut save_file = File::create(save_filename.clone()).expect("can not create save_file");
    save_file.write_all(host_data.as_bytes()).expect("can not write host data to save_file");
    info!("if something breakes please send me the save file under {} which contains the json \
           data from salt",
          save_filename);

    let value: Value = serde_json::from_str(host_data.as_str())
        .expect("can not convert input data to value. have you run the salt command with \
                 --static?");

    let results = get_results(&value);
    trace!("results: {:#?}", results);

    let compressed = get_compressed(results);
    trace!("compressed: {:#?}", compressed);

    print_compressed(compressed, changed);

    std::fs::remove_file(save_filename).expect("can not remove save_file");
}

fn get_results(value: &Value) -> Results {
    if !value.is_object() {
        panic!("value is not an object");
    }

    let mut results: Results = Vec::new();

    for (host, values) in value.as_object().unwrap().iter() {
        trace!("host: {:#?}", host);
        trace!("values: {:#?}", values);

        let retcode: Retcode = match values.get("retcode") {
            Some(o) => {
                o.as_u64()
                    .expect("return code is not a number")
                    .into()
            }
            None => {
                warn!("host {} does not have a return code", host);
                Retcode::Failure
            }
        };


        match *values.get("ret")
            .expect("no ret to parse from value") {
            Value::Null => {
                panic!("do not know how to match null to a result");
            }
            Value::Bool(r) => {
                let (command, result, output) = if r {
                    (None, Some("true".to_string()), None)
                } else {
                    (None, Some("false".to_string()), None)
                };

                results.push(Result {
                    command: command,
                    host: host.clone(),
                    output: output,
                    result: result,
                    retcode: retcode,
                });
            }
            Value::Number(_) => unimplemented!(),
            Value::String(ref r) => {
                results.push(Result {
                    host: host.clone(),
                    result: Some(r.clone()),
                    retcode: retcode,
                    ..Result::default()
                });
            }

            Value::Array(ref r) => {
                let values: Vec<_> = r.iter()
                    .map(|v| v.as_str().expect("can not convert the array value to a string"))
                    .collect();

                results.push(Result {
                    host: host.clone(),
                    result: Some(values.join("\n").to_string()),
                    retcode: retcode.clone(),
                    ..Result::default()
                });
            }
            Value::Object(ref r) => {
                if r.is_empty() {
                    results.push(Result {
                        host: host.clone(),
                        retcode: retcode.clone(),
                        ..Result::default()
                    });
                }

                for (command, command_result) in r.iter() {
                    trace!("command: {:#?}", command);
                    trace!("command_result: {:#?}", command_result);

                    let result = command_result.get("comment")
                        .expect("no comment for the parsed command found")
                        .as_str()
                        .expect("can not convert comment of command to string");

                    let output = match command_result.get("changes") {
                        Some(r) => {
                            match r.get("diff") {
                                Some(d) => {
                                    Some(d.as_str()
                                        .expect("can not convert diff of changes to string")
                                        .to_string())
                                }
                                None => None,
                            }
                        }
                        None => None,
                    };

                    results.push(Result {
                        command: Some(command.to_string()),
                        host: host.clone(),
                        output: output,
                        result: Some(result.to_string()),
                        retcode: retcode.clone(),
                    });
                }
            }
        };
    }

    results
}

fn get_compressed(results: Results) -> DataMap<Result, Vec<String>> {
    // compress output by changeing the hostname to the same value for all results and then just
    // adding all hosts with that value to the map.
    let mut compressed: DataMap<Result, Vec<String>> = DataMap::new();
    for result in results {
        let mut result_no_host = result.clone();
        result_no_host.host = String::new();

        compressed.entry(result_no_host).or_insert_with(Vec::new).push(result.host);
    }

    compressed
}

fn print_compressed(compressed: DataMap<Result, Vec<String>>, changed: bool) {
    let mut unchanged = 0;
    for (result, hosts) in compressed {
        // continue if we only want to print out changes and there are none and the command was a
        // success
        // TODO: make this a filter of the map
        if changed && !result.output.is_some() && result.retcode.is_success() {
            unchanged = unchanged + 1;
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
