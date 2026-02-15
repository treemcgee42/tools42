;;; tally42.el --- TODO                              -*- lexical-binding: t; -*-

;; Copyright (C) 2026  Varun Malladi

;; Author: Varun Malladi <varun.malladi@gmail.com>
;; Keywords: tools

(require 'widget)
(eval-when-compile
  (require 'wid-edit))

(defun tally42-add-transaction ()
  "TODO"
  (interactive)
  (switch-to-buffer "*tally42-add-transaction*")
  (kill-all-local-variables)
  (let ((inhibit-read-only t))
    (erase-buffer))
  (remove-overlays)
  (widget-insert "Add transaction:\n\n")
  ;; Currency and amount can be on the same line.
  (widget-insert "Amount:  ")
  (widget-create 'menu-choice
                 :tag "Currency"
                 :format "%[%v%]"
                 :value "usd"
                 :help-echo "Click to select currency."
                 ;; NB you need explicit formatting on each item to avoid automatic
                 ;; newlines.
                 '(item :tag "USD" :format "%t" :value "usd")
                 '(item :tag "GBP" :format "%t" :value "gbp"))
  (widget-create 'editable-field :size 15 :format " %v\n")
  (widget-create 'menu-choice
                 :tag "Account"
                 :help-echo "Type or click to select an account."
                 :value "(type or select)"
                 :format "%[%t%]: %v"
                 '(editable-field :menu-tag "Custom"
                                  :format "%v"
                                  :value "(type or select)")
                 '(item :tag "Temp account 1" :format "%t" :value "tmp-1")
                 '(item :tag "Temp account 2" :format "%t" :value "tmp-2"))
  (widget-insert "\n")
  (widget-create 'checkbox :format "Split? %[%v%]" nil)
  
  (use-local-map widget-keymap)
  (widget-setup))

(provide 'tally42)
