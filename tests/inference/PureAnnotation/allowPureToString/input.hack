class A {
    public function __toString()[] {
        return "bar";
    }
}

function foo(string $s, A $a)[] : string {
    if ($a == $s) {}
    return $s;
}