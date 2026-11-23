use namespace HH\Lib\Str;

final class A {
    public string $s = '';
    const string C = 'c';
}

function plain(string $s): bool {
    return Str\is_empty($s);
}

function negated(string $s): bool {
    return !Str\is_empty($s);
}

function negated_with_space(string $s): bool {
    return ! \HH\Lib\Str\is_empty($s);
}

function in_condition(string $s, string $t): void {
    if (Str\is_empty($s) && !Str\is_empty($t)) {
        echo 'a';
    }
    if (!Str\is_empty($s) || Str\is_empty($t)) {
        echo 'b';
    }
    invariant(!Str\is_empty($s), 'must not be empty');
    $x = Str\is_empty($s) ? 1 : 2;
    $y = $x === 1 ? Str\is_empty($t) : !Str\is_empty($t);
    echo $x;
    echo $y ? 'y' : 'n';
}

function in_comparison(string $s): bool {
    return Str\is_empty($s) === false;
}

function double_negation(string $s): bool {
    return !!Str\is_empty($s);
}

function operands(A $a, ?string $n, string $s, string $t): void {
    $b1 = Str\is_empty($a->s);
    $b2 = Str\is_empty(A::C);
    $b3 = Str\is_empty($s.$t);
    $b4 = Str\is_empty($n ?? '');
    $b5 = Str\is_empty(Str\trim($s));
    $b6 = Str\is_empty($s === 'a' ? $s : $t);
    echo $b1 ? 'a' : 'b';
    echo $b2 ? 'a' : 'b';
    echo $b3 ? 'a' : 'b';
    echo $b4 ? 'a' : 'b';
    echo $b5 ? 'a' : 'b';
    echo $b6 ? 'a' : 'b';
}

function nullable(?string $s): bool {
    return Str\is_empty($s);
}
