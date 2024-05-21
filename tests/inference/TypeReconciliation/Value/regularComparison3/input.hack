final class A {
    const B = 1;
    const C = 2;

}
function foo(string $s1, string $s2, ?int $i) : string {
    if ($i !== A::B && $i !== A::C) {}

    return $s2;
}