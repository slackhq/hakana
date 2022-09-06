function maybeGetA(): ?A {
    return rand(0, 1) ? new B() : new C();
}

abstract class A {}

class B extends A {
    public function bar(): void {
        $a = maybeGetA();
        if ($a is this) {
            $b = $a;
        }
        $a = $a ?as this;
    }
}

class C extends A {}