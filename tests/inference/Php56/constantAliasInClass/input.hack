namespace Name\Space {
    const FOO = 42;
}

namespace Noom\Spice {
    use const Name\Space\FOO;

    class A {
        public function fooFoo(): void {
            echo FOO . "\n";
            echo \Name\Space\FOO;
        }
    }
}