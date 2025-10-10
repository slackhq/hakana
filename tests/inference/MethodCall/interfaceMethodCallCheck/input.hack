interface A {
    public function foo() : void;
}

interface B extends A {
    public function foo(string $a = "") : void;
}

final class C implements B {
    <<__Override>>
    public function foo(string $a = "") : void {}
}

function takesWithoutArguments(A $a) : void {
    if ($a is B) {
       $a->foo("");
    }
}

takesWithoutArguments(new C());