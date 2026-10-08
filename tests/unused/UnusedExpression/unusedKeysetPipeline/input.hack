function discarded_keys(vec<string> $values): vec<string> {
    $discarded = Keyset\union($values, keyset['extra']) |> Vec\keys($$);
    return $values;
}

function discarded_map(vec<string> $values): vec<string> {
    $discarded = Keyset\map($values, $value ==> Str\uppercase($value));
    return $values;
}

function unselected_value(string $needed, string $unused): string {
    $values = dict['used' => $needed, 'unused' => $unused];
    return $values['used'];
}
