newtype some_alias = int;

function takesVecTuple(vec<(some_alias, int)> $foo): void {}

function bar(vec<(some_alias, int)> $existing, some_alias $i, $a): void {
    $existing[] = tuple($i, $a);
    takesVecTuple($existing);
}