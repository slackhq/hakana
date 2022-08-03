
function foo(vec<string> $tokens): vec<dict<string, mixed>> {
    $out = vec[];
    foreach ($tokens as $i => $token) {
        if ($token[0] == '"') {
            $out[$i - 1]['value'] .= $token;

            $out[] = shape(
                'type' => 'i',
                'value' => $token,
            );
        } else {
            $out[] = shape(
                'type' => 'b',
                'value' => $token,
            );
        }
    }
    return $out;
}