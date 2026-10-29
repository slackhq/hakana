enum MetadataKey: string as string {
    FIRST = 'first';
    SECOND = 'second';
}

function identity<T>(T $value): T {
    return $value;
}

function sanitize(dict<string, mixed> $metadata): dict<string, mixed> {
    $out = dict[];
    foreach ($metadata as $key => $value) {
        if ($key === MetadataKey::FIRST) {
            $out[$key] = true;
        } else if ($key === MetadataKey::SECOND) {
            $out[$key] = 'second';
        } else {
            $out[$key] = false;
        }
    }
    return identity($out);
}

enum OpaqueKey: string {
    FIRST = 'first';
}

function opaque(OpaqueKey $key): string {
    return identity($key);
}
