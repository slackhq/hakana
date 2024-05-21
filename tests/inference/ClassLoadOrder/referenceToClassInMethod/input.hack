final class A {
    public function b(A $b): void {
        $b->b(new A());
    }
}