final class A {
    public int $id = 0;
}
final class B {
    public function foo() : void {}
}

function foo(A $a, dict<int, B> $arr): void {
    if (!isset($arr[$a->id])) {
        $arr[$a->id] = new B();
    }
    $arr[$a->id]->foo();
}