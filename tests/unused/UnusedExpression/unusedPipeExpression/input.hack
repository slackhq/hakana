function foo(): string {
    return 'foo' |> 'a';
}

function bar(): string {
    return 'foo' |> 'a' |> $$;
}