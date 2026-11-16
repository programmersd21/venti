
use builtin;
use str;

set edit:completion:arg-completer[venti] = {|@words|
    fn spaces {|n|
        builtin:repeat $n ' ' | str:join ''
    }
    fn cand {|text desc|
        edit:complex-candidate $text &display=$text' '(spaces (- 14 (wcswidth $text)))$desc
    }
    var command = 'venti'
    for word $words[1..-1] {
        if (str:has-prefix $word '-') {
            break
        }
        set command = $command';'$word
    }
    var completions = [
        &'venti'= {
            cand -h 'Print help (see more with ''--help'')'
            cand --help 'Print help (see more with ''--help'')'
            cand -V 'Print version'
            cand --version 'Print version'
            cand server 'internal daemon server mode (hidden)'
            cand new 'create and attach to new session'
            cand attach 'attach to existing session'
            cand ls 'list all running sessions'
            cand kill 'kill a session'
            cand kill-server 'stop the background daemon'
            cand send 'send keys or commands to pane'
            cand run 'run a command against the session (same as typing it in the palette)'
            cand paste 'paste to focused pane'
            cand config 'manage venti configuration'
            cand doctor 'check terminal: TERM, colors, graphics'
            cand completions 'output shell completions (bash|fish|zsh|elvish)'
            cand man 'display man page'
        }
        &'venti;server'= {
            cand -s 'session name to start server for'
            cand --session 'session name to start server for'
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'venti;new'= {
            cand -s 'session name to create (default: default)'
            cand --session 'session name to create (default: default)'
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'venti;attach'= {
            cand -t 'session name to attach to (default: default)'
            cand --target 'session name to attach to (default: default)'
            cand -d 'disconnect other clients'
            cand --detach-others 'disconnect other clients'
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'venti;ls'= {
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'venti;kill'= {
            cand -t 'session name to terminate'
            cand --target 'session name to terminate'
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'venti;kill-server'= {
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'venti;send'= {
            cand -t 'session name to send to (default: default)'
            cand --target 'session name to send to (default: default)'
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'venti;run'= {
            cand -t 'session name to target (default: default)'
            cand --target 'session name to target (default: default)'
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'venti;paste'= {
            cand -t 'session name to paste to (default: default)'
            cand --target 'session name to paste to (default: default)'
            cand --stdin 'read from stdin instead of clipboard'
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'venti;config'= {
            cand -h 'Print help'
            cand --help 'Print help'
            cand init 'create default ~/.config/venti/config.toml'
            cand check 'check config file for errors'
            cand path 'show config file path'
        }
        &'venti;config;init'= {
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'venti;config;check'= {
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'venti;config;path'= {
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'venti;doctor'= {
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'venti;completions'= {
            cand -h 'Print help (see more with ''--help'')'
            cand --help 'Print help (see more with ''--help'')'
        }
        &'venti;man'= {
            cand -h 'Print help'
            cand --help 'Print help'
        }
    ]
    $completions[$command]
}
