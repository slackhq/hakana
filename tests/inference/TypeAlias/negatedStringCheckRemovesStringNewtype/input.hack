type rich_t = shape('a' => string);

function f(bool $b): shape('text' => string, ?'rich' => rich_t) {
	$text = $b ? make_i18n("hello") : shape('a' => 'x');

	$config = shape('text' => '');
	if ($text is string) {
		// an as-string newtype is still a string
		$config['text'] = $text;
	} else {
		// the newtype is always a string at runtime, so only the
		// shape remains here
		$config['text'] = "fallback";
		$config['rich'] = $text;
	}
	return $config;
}

function g(bool $b): shape('num' => int, ?'rich' => rich_t) {
	$value = $b ? make_id(5) : shape('a' => 'x');

	$config = shape('num' => 0);
	if ($value is int) {
		$config['num'] = $value;
	} else {
		$config['rich'] = $value;
	}
	return $config;
}
