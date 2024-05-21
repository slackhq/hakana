function maybeGetA(): ?A {
    return rand(0, 1) ? new B() : new C();
}

abstract class A {}

final class B extends A {
    public function bar(): void {
        $a = maybeGetA();
        if ($a is this) {
            $b = $a;
        }
        $a = $a ?as this;
    }
}

final class C extends A {}