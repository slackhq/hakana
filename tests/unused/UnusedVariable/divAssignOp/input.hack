function hslToRgb(float $hue): float {
    $hue /= 360;

    return $hue;
}