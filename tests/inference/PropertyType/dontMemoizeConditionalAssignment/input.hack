namespace Foo;

final class A {}

final class B {
    protected ?A $a = null;

    public function test(): void {
        if ($this->a is null) {
            $this->mayBeSetA();
        }
        if ($this->a is A) {
        }
    }

    protected function mayBeSetA(): void {
        if (\mt_rand(0, 1) !== 0) {
            $this->a = new A();
        }
    }
}