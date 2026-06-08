/* HH_FIXME[4030] */
function untyped_ret() {
	return 1;
}

function sum_untyped(): int {
	$xfer = 0;
	$xfer += untyped_ret();
	return $xfer;
}

function modulo_untyped(): int {
	return 100 % untyped_ret();
}

function shift_untyped(): int {
	return untyped_ret() << 8;
}
