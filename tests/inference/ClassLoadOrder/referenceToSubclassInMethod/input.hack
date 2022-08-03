class A {
    public function b(B $b): void {

    }

    public function c(): void {

    }
}

class B extends A {
    public function d(): void {
        $this->c();
    }
}