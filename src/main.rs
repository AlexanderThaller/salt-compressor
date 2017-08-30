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
use std::collections::BTreeSet as DataSet;
use std::fmt;
use std::fs::File;
use std::io::{self, Read};
use std::io::Write;
use std::process;
use time::get_time;

#[cfg(test)]
mod tests;

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
        match input {
            0 => Retcode::Success,
            _ => Retcode::Failure,
        }
    }
}

#[derive(Debug)]
struct Filter {
    command: Regex,
    failed: bool,
    output: Regex,
    result: Regex,
    succeeded: bool,
    unchanged: bool,
}

fn main() {
    let yaml = load_yaml!("cli.yml");
    let matches = App::from_yaml(yaml).version(crate_version!()).get_matches();
    trace!("matches: {:?}", matches);

    {
        let loglevel: LogLevel =
            value_t!(matches, "loglevel", LogLevel).expect("can not parse loglevel from args");
        loggerv::init_with_level(loglevel).expect("can not initialize logger with parsed loglevel");
    }

    let no_save_file = matches.is_present("no_save_file");

    let filter_failed = matches.is_present("filter_failed");
    let filter_succeeded = matches.is_present("filter_succeeded");
    let filter_unchanged = matches.is_present("filter_unchanged");
    let filter_command = value_t!(matches, "filter_command", Regex).expect(
        "can not parse regex from filter_command",
    );
    let filter_result =
        value_t!(matches, "filter_result", Regex).expect("can not parse regex from filter_result");
    let filter_output =
        value_t!(matches, "filter_output", Regex).expect("can not parse regex from filter_output");

    let filter = Filter {
        command: filter_command,
        failed: filter_failed,
        output: filter_output,
        result: filter_result,
        succeeded: filter_succeeded,
        unchanged: filter_unchanged,
    };

    trace!("filter: {:#?}", filter);

    let input_data = {
        let input = matches.value_of("input").expect(
            "can not get input file from args",
        );

        match input {
            "-" => {
                let mut buffer = String::new();
                io::stdin().read_to_string(&mut buffer).expect(
                    "can not read from stdin",
                );
                buffer
            }
            _ => {
                let mut file = File::open(input).expect("can not open input file");
                let mut input = String::new();
                file.read_to_string(&mut input).expect(
                    "can not read input file to string",
                );
                input
            }
        }
    };

    let (host_data, failed_minions) = cleanup_input_data(input_data);

    trace!("input: {}", host_data);

    let value: Value = match serde_json::from_str(host_data.as_str()) {
        Ok(v) => v,
        Err(e) => {
            error!(
                "can not convert input data to value: {}\nhave you run the salt command with \
                 --static?",
                e
            );
            if !no_save_file {
                write_save_file(host_data.as_str());
            }
            process::exit(1)
        }
    };

    trace!("value: {}", value);

    let results = match get_results(&value, failed_minions) {
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

    print_compressed(compressed, &filter);
}

#[derive(Debug)]
enum ResultError {
    ConvertDiffToString,
    ConvertValueToString,
    ReturnCodeNotNumber,
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
            ResultError::RetValueIsNull => write!(f, "ret value is null"),
            ResultError::RetValueIsNumber => write!(f, "ret value is number"),
            ResultError::ValueNotAnObject => write!(f, "value it not an object"),
        }
    }
}

fn get_results(
    value: &Value,
    failed_minions: DataMap<String, &str>,
) -> Result<MinionResults, ResultError> {
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
            Some(r) => r,
            None => values,
        };

        match *ret {
            Value::Null => return Err(ResultError::RetValueIsNull),
            Value::Bool(r) => {
                let result = Some(r.to_string());

                results.push(MinionResult {
                    host: host.clone(),
                    result: result,
                    retcode: retcode,
                    ..MinionResult::default()
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
                    .map(|v| {
                        v.as_str().expect(
                            "can not convert the array value to a string",
                        )
                    })
                    .collect();

                results.push(MinionResult {
                    host: host.clone(),
                    result: Some(values.join("\n").to_string()),
                    retcode: retcode,
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

    for (host, message) in failed_minions {
        results.push(MinionResult {
            host: host,
            retcode: Retcode::Failure,
            output: Some(message.into()),
            ..MinionResult::default()
        });
    }

    Ok(results)
}

fn get_compressed(results: MinionResults) -> DataMap<MinionResult, Vec<String>> {
    // compress output by changeing the hostname to the same value for all results
    // and then just
    // adding all hosts with that value to the map.
    let mut compressed: DataMap<MinionResult, Vec<String>> = DataMap::new();
    for result in results {
        let mut result_no_host = result.clone();
        result_no_host.host = String::new();

        compressed
            .entry(result_no_host)
            .or_insert_with(Vec::new)
            .push(result.host);
    }

    compressed
}

fn print_compressed(compressed: DataMap<MinionResult, Vec<String>>, filter: &Filter) {
    let mut succeeded_hosts = DataSet::default();
    let mut failed_hosts = DataSet::default();

    let mut filter_command = DataSet::default();
    let mut filter_failed = DataSet::default();
    let mut filter_result = DataSet::default();
    let mut filter_output = DataSet::default();
    let mut filter_succeeded = DataSet::default();
    let mut filter_unchanged = 0;

    for (result, hosts) in compressed {
        // continue if we only want to print out changes and there are none and the
        // command was a
        // success
        // TODO: make this a filter of the map
        if filter.succeeded && !result.retcode.is_success() {
            for host in hosts {
                filter_failed.insert(host);
            }
            continue;
        }

        if filter.failed && result.retcode.is_success() {
            for host in hosts {
                filter_succeeded.insert(host);
            }
            continue;
        }

        if filter.unchanged && !result.output.is_some() && result.retcode.is_success() {
            filter_unchanged += 1;
            continue;
        }

        if result.command.is_some() &&
            !filter.command.is_match(
                result.command.clone().unwrap().as_str(),
            )
        {
            for host in hosts {
                filter_command.insert(host);
            }
            continue;
        }

        if result.result.is_some() &&
            !filter.result.is_match(
                result.result.clone().unwrap().as_str(),
            )
        {
            for host in hosts {
                filter_result.insert(host);
            }
            continue;
        }

        if result.output.is_some() &&
            !filter.output.is_match(
                result.output.clone().unwrap().as_str(),
            )
        {
            for host in hosts {
                filter_output.insert(host);
            }
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
                    println!(
                        "{}",
                        format!("COMMAND: {}", result.command.clone().unwrap()).purple()
                    );
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
                Retcode::Success => {
                    for host in hosts {
                        succeeded_hosts.insert(host);
                    }
                    println!("{}{}", "RETURN CODE: ".yellow(), "Success".green())
                }
                Retcode::Failure => {
                    for host in hosts {
                        failed_hosts.insert(host);
                    }
                    println!("{}{}", "RETURN CODE: ".yellow(), "Failure".red())
                }
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

    println!("");

    print_filter_statistics("command", filter_command.len());
    print_filter_statistics("result", filter_result.len());
    print_filter_statistics("output", filter_output.len());
    print_filter_statistics("failed", filter_failed.len());
    print_filter_statistics("succeeded", filter_succeeded.len());
    print_filter_statistics("changed", filter_unchanged);

    info!(
        "succeeded host{}: {}",
        if succeeded_hosts.len() > 1 || succeeded_hosts.is_empty() {
            "s"
        } else {
            ""
        },
        succeeded_hosts.len()
    );
    info!(
        "failed host{}: {}",
        if failed_hosts.len() > 1 || failed_hosts.is_empty() {
            "s"
        } else {
            ""
        },
        failed_hosts.len()
    );
}

fn print_filter_statistics(stats: &str, count: usize) {
    info!(
        "filtered {} state{}: {}",
        stats,
        if count > 1 || count == 0 { "s" } else { "" },
        count
    );
}

fn write_save_file(host_data: &str) {
    let save_filename = format!("/tmp/salt-compressor_{}.json", get_time().sec);
    let mut save_file = File::create(save_filename.clone()).expect("can not create save_file");
    save_file.write_all(host_data.as_bytes()).expect(
        "can not write host data to save_file",
    );
    info!(
        "please send me the save file under {} which contains the json \
           data from salt",
        save_filename
    );
}

fn cleanup_input_data<'a>(
    input_data: String,
) -> (String, std::collections::BTreeMap<std::string::String, &'a str>) {
    let mut failed_minions = DataMap::default();

    // Cleanup input data from minions that either didnt return or had a duplicate key
    let input_data = {
        // match all hosts that have not returned as they are not in the json data
        // format is normally like "Minion minionid did not respond. No job will be
        // sent."
        let catch_not_returned_minions =
            Regex::new(
                r"(?m)^Minion (\S*) did not respond\. No job will be sent\.$",
            ).expect("regex for catching not returned minions is not valid");

        let errmessage = "Minion did not respond. No job will be sent.";
        for host in catch_not_returned_minions.captures_iter(input_data.as_str()) {
            failed_minions.insert(host[1].to_string(), errmessage);
        }

        let data = catch_not_returned_minions
            .replace_all(input_data.as_str(), "")
            .into_owned();

        // match all hosts that have a duplicate key in the system
        // like "minion minionid was already deleted from tracker, probably a duplicate key"
        let catch_duplicate_key_minions =
            Regex::new(
                r"(?m)^minion (\S*) was already deleted from tracker, probably a duplicate key",
            ).expect("regex for catching duplicate key minions is not valid");

        let errmessage = "Minion was already deleted from tracker, probably a duplicate key.";
        for host in catch_duplicate_key_minions.captures_iter(input_data.as_str()) {
            failed_minions.insert(host[1].to_string(), errmessage);
        }

        let data = catch_duplicate_key_minions
            .replace_all(data.as_str(), "")
            .into_owned();

        data
    };

    let no_return_received = "ERROR: No return received";
    let input_data = if input_data.contains(no_return_received) {
        failed_minions.insert('*'.to_string(), "ERROR: No return received.");
        input_data.replace(no_return_received, "")
    } else {
        input_data
    };

    // clean up hosts that have not returned from the json data
    (input_data, failed_minions)
}
