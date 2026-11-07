# Warn on save-breaking updates without forcing a new save folder

The owner chose warning and acknowledgement over mandatory save-folder isolation: an incompatible release may continue using the existing save folder. The launcher preserves saves during installation and does not convert them, but the new game release may reject or modify those saves after launch; restoring an older installation does not restore save compatibility. See the [release/update contract resolution](https://github.com/synpoox/resurrected-launcher/issues/6) for the decision and its limits.
