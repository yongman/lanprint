"""Small real PDF fixture with a page tree and cross-reference table."""


def make_pdf(pages):
    objects = [b'<< /Type /Catalog /Pages 2 0 R >>']
    kids = ' '.join(f'{index + 3} 0 R' for index in range(pages))
    objects.append(f'<< /Type /Pages /Count {pages} /Kids [{kids}] >>'.encode())
    objects.extend([b'<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] >>'] * pages)
    result = b'%PDF-1.4\n'
    offsets = [0]
    for index, obj in enumerate(objects, 1):
        offsets.append(len(result))
        result += f'{index} 0 obj\n'.encode() + obj + b'\nendobj\n'
    xref = len(result)
    result += f'xref\n0 {len(offsets)}\n0000000000 65535 f \n'.encode()
    result += b''.join(f'{offset:010} 00000 n \n'.encode() for offset in offsets[1:])
    result += f'trailer\n<< /Size {len(offsets)} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n'.encode()
    return result
