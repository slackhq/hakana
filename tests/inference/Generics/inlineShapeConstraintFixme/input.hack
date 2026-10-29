function identity<T>(T $value): T {
    return $value;
}

function suppressed(nonnull $value): shape('fixed' => vec<int>, 'ok' => bool) {
    return identity(shape('fixed' => /* HH_FIXME[4110] */ $value, 'ok' => true));
}

function unsuppressed(nonnull $value): shape('fixed' => vec<int>, 'ok' => bool) {
    return identity(shape('fixed' => $value, 'ok' => true));
}

function another_field(nonnull $value): shape('fixed' => vec<int>, 'ok' => bool) {
    return identity(shape('ok' => 'bad', 'fixed' => /* HH_FIXME[4110] */ $value));
}

function missing_field(nonnull $value): shape('fixed' => vec<int>, 'ok' => bool) {
    return identity(shape('fixed' => /* HH_FIXME[4110] */ $value));
}

function nested(nonnull $value): shape('inner' => shape('fixed' => vec<int>, 'ok' => bool)) {
    return identity(shape('inner' => shape('ok' => true, 'fixed' => /* HH_FIXME[4110] */ $value)));
}

function in_closure(nonnull $value): shape('fixed' => vec<int>, 'ok' => bool) {
    return identity(() ==> shape('fixed' => /* HH_FIXME[4110] */ $value, 'ok' => true))();
}

function outer_fixme(nonnull $value): shape('fixed' => vec<int>, 'ok' => bool) {
    /* HH_FIXME[4110] */
    return identity(shape(
        'ok' => 'bad',
        'fixed' => $value,
    ));
}

function inline_outer_fixme(nonnull $value): shape('fixed' => vec<int>, 'ok' => bool) {
    return identity(/* HH_FIXME[4110] */ shape(
        'ok' => true,
        'fixed' => $value,
    ));
}
