function foo((int, int) $s): void {
	$a = rand(0, 1) !== 0 ? $s : vec[];
    $b = rand(0, 1) !== 0 ? vec[] : $s;
    if ($a) {}
    if ($b) {}
}
