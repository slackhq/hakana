final class C {
    const A = 1;
    const B = -1;
}

const A = 1;
const B = -1;

$i = rand(0, 1) !== 0 ? A : B;
if (rand(0, 1) !== 0) {
    $i = 0;
}

if ($i === A) {
    echo "here";
} else if ($i === B) {
    echo "here";
}

$i = rand(0, 1) !== 0 ? C::A : C::B;

if (rand(0, 1) !== 0) {
    $i = 0;
}

if ($i === C::A) {
    echo "here";
} else if ($i === C::B) {
    echo "here";
}
