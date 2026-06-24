function foo(dict<int, mixed> $dict): dict<int, mixed> {
	if (rand(0, 1) !== 0) {
    	$dict['a'] = 'foo';
    }
    
    return $dict;
}
