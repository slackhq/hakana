namespace {
    abstract class A {
        public static function static_name(): void {
            $static_name = nameof static;
            if ($static_name == 'quux') {
                echo "impossible\n";
            }
        }
    }

    final class B extends A {
        public function foo(bool $something): void {
            $bar = $something ? nameof self : nameof parent;
            if ($bar == 'quux') {
                echo "impossible\n";
            }

            $nameof_c = nameof C;
            if ($nameof_c == 'quux') {
                echo "impossible\n";
            }

            if ($nameof_c == 'C') {
                echo "correct!\n";
            }

            $nameof_d = nameof Foo\D;
            if ($nameof_d == 'D') {
                echo "impossible!\n";
            }
            if ($nameof_d == 'Foo\D') {
                echo "correct!\n";
            }
        }
    }

    final class C {}
}

namespace Foo {
    final class D {
        public function foo(bool $something): void {
            $nameof_c = nameof \C;
            if ($nameof_c == 'Foo\C') {
                echo "impossible\n";
            }
            if ($nameof_c == 'C') {
                echo "correct!\n";
            }

            $nameof_d = nameof D;
            if ($nameof_d == 'D') {
                echo "impossible\n";
            }

            if ($nameof_d == 'Foo\D') {
                echo "correct!\n";
            }
        }
    }
}