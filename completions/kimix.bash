# kimix bash completion
_kimix_completion() {
    local cur prev opts
    COMPREPLY=()
    cur="${COMP_WORDS[COMP_CWORD]}"
    prev="${COMP_WORDS[COMP_CWORD-1]}"

    opts="--agent --allow --always-approve --best-of-n --continue --check --cwd --debug --disable-web-search --fork-session --fullscreen --help --json-schema --leader-socket --model --max-turns --minimal --no-alt-screen --no-memory --no-plan --no-subagents --output-format --permission-mode --resume --version -c -h -m -p -r"

    case "${prev}" in
        --model|-m) COMPREPLY=($(compgen -W "default deepseek-pro deepseek-flash longcat mimo mimo-pro" -- "${cur}")); return 0 ;;
        --permission-mode) COMPREPLY=($(compgen -W "default acceptEdits auto dontAsk bypassPermissions plan" -- "${cur}")); return 0 ;;
        --output-format) COMPREPLY=($(compgen -W "plain json streaming-json" -- "${cur}")); return 0 ;;
        *) COMPREPLY=($(compgen -W "${opts}" -- "${cur}")); return 0 ;;
    esac
}
complete -F _kimix_completion kimix
