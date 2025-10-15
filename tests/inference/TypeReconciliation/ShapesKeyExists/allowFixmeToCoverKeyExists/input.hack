function blah(shape('val' => int) $x): void {
    if (Shapes::keyExists($x, 'val')) {}
}

function blat(vec<shape('val' => int)> $y): void {
    foreach ($y as $x) {
        if (
            /*HH_FIXME[4249]*/
            Shapes::keyExists($x, 'val') &&
            $x['val'] == 5 &&
            \rand(0, 1)
        ) {}
    }
}
