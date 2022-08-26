function foo(mixed $m): dict<string, string> {
    return HH\Lib\Dict\from_keys(
        $m,
        $l ==> takesString($l),
    );
}

function takesString(string $s): string {
    return 'hello ' . $s;
}