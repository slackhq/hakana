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
    }
}

final class C {}