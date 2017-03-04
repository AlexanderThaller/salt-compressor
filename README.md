# salt-compressor
Compress the output of saltstack-runs to make it easier to review changes that happen to a lot of servers.

# Usage
```
salt-compressor 0.2.0
Compress the output of saltstack-runs to make it easier to review changes that happen to a lot of servers.

USAGE:
    salt-compressor [FLAGS] [OPTIONS] --input <path>

FLAGS:
    -c, --changed
            Only print states that have outputs
    -h, --help
            Prints help information
    -n, --no_save_file
            Do not write save file on error
    -V, --version
            Prints version information

OPTIONS:
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
