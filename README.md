# salt-compressor
Compress the output of saltstack-runs to make it easier to review changes that happen to a lot of servers.

# Example
```
salt '*' state.highstate -b 10 --static --out json test=true | salt-compressor -i -
```

The `--static` and `--json` flags are important. `static` will output a much
easier to parse format. `json` will of course output everything in the JSON
format.
