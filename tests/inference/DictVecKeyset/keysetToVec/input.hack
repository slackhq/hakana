function keyset_to_vec<T as arraykey>(keyset<T> $keyset): vec<T> {
	return vec($keyset);
}

function takesKeysetString(keyset<string> $k): vec<string> {
    return keyset_to_vec($k);
}