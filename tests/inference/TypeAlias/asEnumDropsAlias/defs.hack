newtype my_interop_string as string = string;

function make_interop(string $s): my_interop_string {
	return $s;
}
