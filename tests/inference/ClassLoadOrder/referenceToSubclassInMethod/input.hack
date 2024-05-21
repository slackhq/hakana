abstract class A {
    public function b(B $b): void {

    }

    public function c(): void {

    }
}

final class B extends A {
    public function d(): void {
        $this->c();
    }
}