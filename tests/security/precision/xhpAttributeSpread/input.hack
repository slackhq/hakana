use type Facebook\XHP\HTML\form;

$action = (string)HH\global_get('_SERVER')['REQUEST_URI'];
$source = <form action={$action} />;
$spread = <form {...$source} />;
$safe = <form action={"/submit"} />;
