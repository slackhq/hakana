final class A {
    public function foo(): void {
        $this->bar();
    }

    private function bar(): void {
        echo 'a';
        echo 'b';
    }
}