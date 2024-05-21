final class A extends B {}

abstract class B {
    public function fooFoo(): void {
        $a = new A();
        $a->barBar();
    }

    protected function barBar(): void {
        echo "hello";
    }
}