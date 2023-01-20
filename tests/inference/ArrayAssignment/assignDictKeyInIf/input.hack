function foo(dict<int, mixed> $dict): dict<int, mixed> {
	if (rand(0, 1)) {
    	$dict['a'] = 'foo';
    }
    
    return $dict;
}