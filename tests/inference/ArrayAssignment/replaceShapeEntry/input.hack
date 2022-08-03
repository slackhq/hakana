function foo(): shape("a" => string) {
	$arr = shape('a' => 4);
    $arr['a'] = $arr['a'] . "foo";
    return $arr;
}