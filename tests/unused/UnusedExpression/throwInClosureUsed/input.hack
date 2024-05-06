function foo(): void {
    $fn_fail = () ==> {
		throw new Exception("bad");
	};
    $fn_fail();
}