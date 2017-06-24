# salt-compressor
Compress the output of saltstack-runs to make it easier to review changes that happen to a lot of servers.

[![TravisCI Build Status](https://travis-ci.org/AlexanderThaller/salt-compressor.svg?branch=master)](https://travis-ci.org/AlexanderThaller/salt-compressor)

# Usage
```
salt-compressor 0.4.0
Alexander Thaller <alexander.thaller@trivago.com>
Compress the output of saltstack-runs to make it easier to review changes that happen to a lot of servers.

USAGE:
    salt-compressor [FLAGS] [OPTIONS] --input <path>

FLAGS:
    -F, --filter_failed
            Only print states that failed

    -S, --filter_succeeded
            Only print states that succeeded

    -U, --filter_unchanged
            Only print states that have outputs

    -h, --help
            Prints help information

    -n, --no_save_file
            Do not write save file on error

    -V, --version
            Prints version information


OPTIONS:
    -C, --filter_command <regex>
            Only print states that have commands that match the given regex [default: .*]

    -O, --filter_output <regex>
            Only print states that have outputs that match the given regex [default: .*]

    -R, --filter_result <regex>
            Only print states that have results that match the given regex [default: .*]

    -i, --input <path>
            Path to the input file. If input is '-' read from stdin

    -l, --loglevel <level>
            Loglevel to run under [default: info]  [values: trace, debug, info, warn, error]
```

# Example
```
salt '*' state.highstate -b 10 --static --out json test=true | salt-compressor -i -
```

The `--static` and `--json` flags are important. `static` will output a much
easier to parse format. `json` will of course output everything in the JSON
format.
