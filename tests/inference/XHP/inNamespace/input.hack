use namespace Facebook\XHP\Core as x;

function fetch(x\node $xhp): string {
    return HH\Asio\join($xhp->toStringAsync());
}

