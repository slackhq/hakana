enum SomeEnum: string as string {
	A = 'a';
	B = 'b';
	C = 'c';
}

function foo(dict<string, mixed> $dict): dict<string, mixed> {
	if (rand(0, 1) !== 0) {
    	$dict[SomeEnum::A] = 'foo';
    }
    
    return $dict;
}
