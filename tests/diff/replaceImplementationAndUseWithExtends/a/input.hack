abstract class A {
    private function __construct(private int $i) {}

    public function returnSelf() : A {
        return new A(5);
    }
}

final class B extends A {
    public function returnSelf(): A {
        return parent::returnSelf();
    }
}

<<__EntryPoint>>
function main(): void {
    B::returnSelf();
}
