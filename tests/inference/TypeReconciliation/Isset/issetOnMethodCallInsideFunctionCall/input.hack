final class C {
    public function foo() : ?string {
        return null;
    }
}

function foo(C $c) : void {
    strlen($c->foo() ?? "");
}