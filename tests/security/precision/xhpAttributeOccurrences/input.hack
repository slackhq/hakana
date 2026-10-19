use type Facebook\XHP\HTML\form;

$action = (string)HH\global_get('_SERVER')['REQUEST_URI'];
$first = <form action={$action} />;
$second = <form action={$action} />;
$safe = <form action={"/submit"} />;
