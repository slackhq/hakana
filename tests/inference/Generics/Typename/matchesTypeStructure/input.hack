function matches_type_structure<T>(TypeStructure<T> $ts, mixed $value): T {
}

final class A {
    const type T_B = shape('c' => string);
}

function foo(mixed $untyped): shape('c' => string) {
    $type_structure = type_structure(A::class, 'T_B');
    $typed = matches_type_structure($type_structure, $untyped);
    return $typed;
}