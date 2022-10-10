namespace Foo;

class A {}

class B {
    protected ?A $a = null;

    public function test(): void {
        if (!$this->a) {
            $this->mayBeSetA();
        }
        if ($this->a is A) {
        }
    }

    protected function mayBeSetA(): void {
        if (\mt_rand(0, 1)) {
            $this->a = new A();
        }
    }
}