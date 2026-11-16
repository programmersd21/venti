# Print an optspec for argparse to handle cmd's options that are independent of any subcommand.
function __fish_venti_global_optspecs
    string join \n h/help V/version
end

function __fish_venti_needs_command
    # Figure out if the current invocation already has a command.
    set -l cmd (commandline -opc)
    set -e cmd[1]
    argparse -s (__fish_venti_global_optspecs) -- $cmd 2>/dev/null
    or return
    if set -q argv[1]
        # Also print the command, so this can be used to figure out what it is.
        echo $argv[1]
        return 1
    end
    return 0
end

function __fish_venti_using_subcommand
    set -l cmd (__fish_venti_needs_command)
    test -z "$cmd"
    and return 1
    contains -- $cmd[1] $argv
end

complete -c venti -n "__fish_venti_needs_command" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c venti -n "__fish_venti_needs_command" -s V -l version -d 'Print version'
complete -c venti -n "__fish_venti_needs_command" -f -a "server" -d 'internal daemon server mode (hidden)'
complete -c venti -n "__fish_venti_needs_command" -f -a "new" -d 'create and attach to new session'
complete -c venti -n "__fish_venti_needs_command" -f -a "attach" -d 'attach to existing session'
complete -c venti -n "__fish_venti_needs_command" -f -a "ls" -d 'list all running sessions'
complete -c venti -n "__fish_venti_needs_command" -f -a "kill" -d 'kill a session'
complete -c venti -n "__fish_venti_needs_command" -f -a "kill-server" -d 'stop the background daemon'
complete -c venti -n "__fish_venti_needs_command" -f -a "send" -d 'send keys or commands to pane'
complete -c venti -n "__fish_venti_needs_command" -f -a "run" -d 'run a command against the session (same as typing it in the palette)'
complete -c venti -n "__fish_venti_needs_command" -f -a "paste" -d 'paste to focused pane'
complete -c venti -n "__fish_venti_needs_command" -f -a "config" -d 'manage venti configuration'
complete -c venti -n "__fish_venti_needs_command" -f -a "doctor" -d 'check terminal: TERM, colors, graphics'
complete -c venti -n "__fish_venti_needs_command" -f -a "completions" -d 'output shell completions (bash|fish|zsh|elvish)'
complete -c venti -n "__fish_venti_needs_command" -f -a "man" -d 'display man page'
complete -c venti -n "__fish_venti_using_subcommand server" -s s -l session -d 'session name to start server for' -r
complete -c venti -n "__fish_venti_using_subcommand server" -s h -l help -d 'Print help'
complete -c venti -n "__fish_venti_using_subcommand new" -s s -l session -d 'session name to create (default: default)' -r
complete -c venti -n "__fish_venti_using_subcommand new" -s h -l help -d 'Print help'
complete -c venti -n "__fish_venti_using_subcommand attach" -s t -l target -d 'session name to attach to (default: default)' -r
complete -c venti -n "__fish_venti_using_subcommand attach" -s d -l detach-others -d 'disconnect other clients'
complete -c venti -n "__fish_venti_using_subcommand attach" -s h -l help -d 'Print help'
complete -c venti -n "__fish_venti_using_subcommand ls" -s h -l help -d 'Print help'
complete -c venti -n "__fish_venti_using_subcommand kill" -s t -l target -d 'session name to terminate' -r
complete -c venti -n "__fish_venti_using_subcommand kill" -s h -l help -d 'Print help'
complete -c venti -n "__fish_venti_using_subcommand kill-server" -s h -l help -d 'Print help'
complete -c venti -n "__fish_venti_using_subcommand send" -s t -l target -d 'session name to send to (default: default)' -r
complete -c venti -n "__fish_venti_using_subcommand send" -s h -l help -d 'Print help'
complete -c venti -n "__fish_venti_using_subcommand run" -s t -l target -d 'session name to target (default: default)' -r
complete -c venti -n "__fish_venti_using_subcommand run" -s h -l help -d 'Print help'
complete -c venti -n "__fish_venti_using_subcommand paste" -s t -l target -d 'session name to paste to (default: default)' -r
complete -c venti -n "__fish_venti_using_subcommand paste" -l stdin -d 'read from stdin instead of clipboard'
complete -c venti -n "__fish_venti_using_subcommand paste" -s h -l help -d 'Print help'
complete -c venti -n "__fish_venti_using_subcommand config; and not __fish_seen_subcommand_from init check path" -s h -l help -d 'Print help'
complete -c venti -n "__fish_venti_using_subcommand config; and not __fish_seen_subcommand_from init check path" -f -a "init" -d 'create default ~/.config/venti/config.toml'
complete -c venti -n "__fish_venti_using_subcommand config; and not __fish_seen_subcommand_from init check path" -f -a "check" -d 'check config file for errors'
complete -c venti -n "__fish_venti_using_subcommand config; and not __fish_seen_subcommand_from init check path" -f -a "path" -d 'show config file path'
complete -c venti -n "__fish_venti_using_subcommand config; and __fish_seen_subcommand_from init" -s h -l help -d 'Print help'
complete -c venti -n "__fish_venti_using_subcommand config; and __fish_seen_subcommand_from check" -s h -l help -d 'Print help'
complete -c venti -n "__fish_venti_using_subcommand config; and __fish_seen_subcommand_from path" -s h -l help -d 'Print help'
complete -c venti -n "__fish_venti_using_subcommand doctor" -s h -l help -d 'Print help'
complete -c venti -n "__fish_venti_using_subcommand completions" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c venti -n "__fish_venti_using_subcommand man" -s h -l help -d 'Print help'
