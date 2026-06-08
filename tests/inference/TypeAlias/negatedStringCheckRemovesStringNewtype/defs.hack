newtype my_i18n_string as string = string;

function make_i18n(string $s): my_i18n_string {
	return $s;
}

newtype my_id as int = int;

function make_id(int $i): my_id {
	return $i;
}
