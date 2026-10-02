---
id: 01JBQ7Z9W6K4M2N8P0R3T5V7XA
title: Constructs the editor does not model
description: A page of the shapes ED-03(c) calls opaque, so the editor's tests have one.
---

ED-03(c) says a construct the visual editor does not model becomes an opaque
node carrying its source bytes. Those shapes were absent from every fixture
page, so `renderOpaque` and the source popover had no segmentation to be tested
against and the browser test for them could only skip.

<div class="legacy-callout" data-from="the old site">
  <p>Raw HTML is published exactly as written, so the editor shows the bytes.</p>
</div>

{# A template comment never reaches a reader, and the editor does not model it. #}

Prose between them, so the opaque runs do not merge into one segment.

:::note
A container that is never closed. The editor cannot tell where it ends, which is
the one shape of the four that is a mistake rather than a choice.
