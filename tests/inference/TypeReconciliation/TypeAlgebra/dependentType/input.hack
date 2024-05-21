final class A {
    public function isValid() : bool {
        return (bool) rand(0, 1);
    }

    public function foo() : void {}
}

function takesA(?A $a) : void {
    $is_valid_a = $a && $a->isValid();

    if ($is_valid_a) {
        $a->foo();
    }
}