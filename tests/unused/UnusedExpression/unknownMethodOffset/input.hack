function foo(mixed $m): dict<string, string> {
    $a = dict[];
    
    if (rand(0, 1)) {
        /* HAKANA_FIXME[MixedMethodCall] */
    	$a[$m->foo()] = '5';
    } else {
        $a['m'] = 'a';
    }
    
    /* HAKANA_FIXME[LessSpecificNestedAnyReturnStatement] */
    return $a;
}