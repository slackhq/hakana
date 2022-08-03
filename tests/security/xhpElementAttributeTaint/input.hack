use type Facebook\XHP\HTML\span;

function foo(string $a) {
    $b = do_trim($a);
    return <span style={"background: " . $b}>Hello</a>;
}

function do_trim(string $str): string {
	return Str\trim($str);
}

foo($_GET['background']);