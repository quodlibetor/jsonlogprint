# jsonlogprint

Logs in JSON format are great for machines but super annoying for people.

`jsonlogprint` formats JSON logs for humans, ideally actually better than
plain text logs through judicious use of syntax and color.

For example, these log lines:

```text
{"timestamp": 1729811012050, "level": "WARN", "message": "hello there"}
{"timestamp": 1729811033000, "level": "TRACE", "msg": "a message", "prop": "interestingProperty", "stacktrace": "foo\nbar\nblah", "something": "info"}
{"timestamp": 1729810953000, "level": "INFO", "msg": "a message", "prop": {"field": {"a": {"complex": {"really": {"really": "complex"}}}, "b": 2}}, "something": "foo"}
```

get formatted like this:

![screenshot of jsonlogprint output](static/screenshot.png)
