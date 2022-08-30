use namespace HH\Lib\{Regex};

function foo(string $s): ?shape("subdomain" => string, ...) {
    return Regex\first_match(
		$s,
		re"/^https:\/\/(?<subdomain>.+?)?\.example\.com\//",
	);
}