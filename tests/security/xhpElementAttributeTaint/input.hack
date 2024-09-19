use type Facebook\XHP\HTML\span;

function foo(string $a) {
    $b = do_trim($a);
    return <span style={"background: " . $b}>Hello</span>;
}

function do_trim(string $str): string {
	return Str\trim($str);
}

foo(HH\global_get('_GET')['background']);