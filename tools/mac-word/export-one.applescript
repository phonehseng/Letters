-- exports one docx to pdf with microsoft word for mac.
-- arguments: the docx path, then the pdf path (both posix paths inside
-- word's container folder, so word can read and write them).
-- the document is closed without saving, so the kit's file never changes.
on run argv
	set docxPath to item 1 of argv
	set pdfPath to item 2 of argv
	tell application "Microsoft Word"
		-- a long document may take a while to lay out
		with timeout of 300 seconds
			open (POSIX file docxPath)
			set theDocument to active document
			save as theDocument file name pdfPath file format format PDF
			close theDocument saving no
		end timeout
	end tell
end run
