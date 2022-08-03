class A extends B {}

class B {
    public function fooFoo(): void {
        $a = new A();
        $a->barBar();
    }

    protected function barBar(): void {
        echo "hello";
    }
}