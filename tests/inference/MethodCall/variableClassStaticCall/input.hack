class C {
    public static function f(int $u):void {}
}

function f(): void {
	$u = "a";
    $r = vec[C::class];
    foreach ($r as $c) {
        $c::f($u);
    }
}