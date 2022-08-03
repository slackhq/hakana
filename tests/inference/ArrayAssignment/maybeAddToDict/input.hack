function foo(dict<string, string> $arr): dict<string, string> {
    $arr['a'] = 'boogaloo';
    foreach (vec['f', 'g'] as $v) {
		$arr[$v] = $arr['d'];
	}
    
    return $arr;
}